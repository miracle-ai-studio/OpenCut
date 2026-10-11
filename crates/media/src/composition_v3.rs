//! Shared schema 3 visual filter graph for frame, range, export and preview.
use crate::{
    CompiledVisualGraphV3, MaskShapeV3, MediaError, MediaResult, MediaTime, ShapePrimitiveV3,
    TimelineExportRequest, VisualEffectV3, VisualTargetKindV3,
};
use std::collections::HashSet;

pub(crate) fn transition_source_handles(
    timeline: &TimelineExportRequest,
    visual: &CompiledVisualGraphV3,
) -> MediaResult<Vec<(usize, f64, f64)>> {
    let mut ranges = Vec::new();
    for transition in &visual.transitions {
        let outgoing = visual
            .targets
            .iter()
            .find(|t| t.target_id == transition.outgoing_clip_id)
            .ok_or_else(|| {
                MediaError::InvalidRequest("crossfade outgoing target is missing".into())
            })?;
        let incoming = visual
            .targets
            .iter()
            .find(|t| t.target_id == transition.incoming_clip_id)
            .ok_or_else(|| {
                MediaError::InvalidRequest("crossfade incoming target is missing".into())
            })?;
        if outgoing.kind != VisualTargetKindV3::VideoClip
            || incoming.kind != VisualTargetKindV3::VideoClip
            || outgoing.index == incoming.index
            || outgoing.z_index != incoming.z_index
        {
            return Err(MediaError::InvalidRequest(
                "crossfade requires adjacent video clips on one layer".into(),
            ));
        }
        let out = timeline.video_clips.get(outgoing.index).ok_or_else(|| {
            MediaError::InvalidRequest("crossfade outgoing clip is out of range".into())
        })?;
        let inc = timeline.video_clips.get(incoming.index).ok_or_else(|| {
            MediaError::InvalidRequest("crossfade incoming clip is out of range".into())
        })?;
        let cut = inc.timeline_start.seconds();
        let half = transition.duration.seconds() / 2.0;
        if transition.duration.ticks <= 0
            || transition.duration.timebase == 0
            || (out.timeline_start.seconds() + out.timeline_duration.seconds() - cut).abs()
                > 0.000_001
            || (out.source_duration.seconds() - out.timeline_duration.seconds()).abs() > 0.000_001
            || (inc.source_duration.seconds() - inc.timeline_duration.seconds()).abs() > 0.000_001
            || out.timeline_duration.seconds() < half
            || inc.timeline_duration.seconds() < half
            || inc.source_start.seconds() < half
            || ![cut, half].iter().all(|v| v.is_finite())
        {
            return Err(MediaError::InvalidRequest(
                "crossfade source handles or cut are invalid".into(),
            ));
        }
        ranges.push((
            outgoing.index,
            out.source_start.seconds(),
            out.source_duration.seconds() + half,
        ));
        ranges.push((
            incoming.index,
            inc.source_start.seconds() - half,
            inc.source_duration.seconds() + half,
        ));
    }
    Ok(ranges)
}

pub(crate) fn build_filter(
    timeline: &TimelineExportRequest,
    visual: &CompiledVisualGraphV3,
    frame_time: Option<MediaTime>,
    range: Option<(MediaTime, MediaTime)>,
) -> MediaResult<String> {
    validate(timeline, visual)?;
    let width = timeline.video.width;
    let height = timeline.video.height;
    let fps = format!(
        "{}/{}",
        timeline.video.frame_rate.numerator, timeline.video.frame_rate.denominator
    );
    // Keep absolute timestamps for layer animation and captions, but do not
    // composite every preceding black frame when seeking into a sequence.
    let window = if let Some(time) = frame_time {
        format!("trim=start={:.9},", time.seconds())
    } else if let Some((start, duration)) = range {
        format!(
            "trim=start={:.9}:duration={:.9},",
            start.seconds(),
            duration.seconds()
        )
    } else {
        String::new()
    };
    let mut chains = vec![format!("[0:v]{window}format=yuv420p[base0]")];
    let mut base = "base0".to_string();
    let mut targets = visual.targets.iter().collect::<Vec<_>>();
    targets.sort_by_key(|target| (target.z_index, target.layer_order));
    for (layer, target) in targets.into_iter().enumerate() {
        let layer_chain_start = chains.len();
        let label = format!("v3layer{layer}");
        let (start, duration) = match target.kind {
            VisualTargetKindV3::VideoClip | VisualTargetKindV3::ImageClip => {
                let (
                    input,
                    stream,
                    source,
                    start,
                    duration,
                    opacity,
                    px,
                    py,
                    sx,
                    sy,
                    rotation,
                    curves,
                    fade,
                ) = if target.kind == VisualTargetKindV3::VideoClip {
                    let clip = &timeline.video_clips[target.index];
                    let rate = clip.source_duration.seconds() / clip.timeline_duration.seconds();
                    if !(0.01..=100.0).contains(&rate) {
                        return Err(MediaError::Unsupported(
                            "video rate is outside supported range".into(),
                        ));
                    }
                    let mut source_start = clip.source_start.seconds();
                    let mut source_end = source_start + clip.source_duration.seconds();
                    let mut start = clip.timeline_start.seconds();
                    let mut duration = clip.timeline_duration.seconds();
                    let mut fade = None;
                    for transition in &visual.transitions {
                        let half = transition.duration.seconds() / 2.0;
                        if transition.outgoing_clip_id == target.target_id {
                            source_end += half;
                            duration += half;
                        }
                        if transition.incoming_clip_id == target.target_id {
                            source_start -= half;
                            start -= half;
                            duration += half;
                            fade = Some((
                                clip.timeline_start.seconds() - half,
                                transition.duration.seconds(),
                            ));
                        }
                    }
                    (
                        1 + target.index,
                        clip.stream.map_or_else(|| "v:0".into(), |n| n.to_string()),
                        format!(
                            "trim=start={source_start:.9}:end={source_end:.9},setpts=(PTS-STARTPTS)/{rate:.12}+{start:.9}/TB"
                        ),
                        start,
                        duration,
                        clip.opacity as f64,
                        clip.position_x,
                        clip.position_y,
                        clip.scale_x,
                        clip.scale_y,
                        clip.rotation_degrees,
                        &clip.curves,
                        fade,
                    )
                } else {
                    let clip = &timeline.image_clips[target.index];
                    (
                        1 + timeline.video_clips.len() + target.index,
                        clip.stream.map_or_else(|| "v:0".into(), |n| n.to_string()),
                        format!(
                            "setpts=PTS-STARTPTS+{:.9}/TB",
                            clip.timeline_start.seconds()
                        ),
                        clip.timeline_start.seconds(),
                        clip.timeline_duration.seconds(),
                        clip.opacity as f64,
                        clip.position_x,
                        clip.position_y,
                        clip.scale_x,
                        clip.scale_y,
                        clip.rotation_degrees,
                        &clip.curves,
                        None,
                    )
                };
                let identity_geometry = px == 0.0
                    && py == 0.0
                    && sx == 1.0
                    && sy == 1.0
                    && rotation == 0.0
                    && curves.position_x.segments.is_empty()
                    && curves.position_y.segments.is_empty()
                    && curves.scale_x.segments.is_empty()
                    && curves.scale_y.segments.is_empty()
                    && curves.rotation_degrees.segments.is_empty();
                let constant_opacity = curves.opacity.segments.is_empty() && fade.is_none();
                let static_opacity = opacity;
                // Transition handles expose extra source frames before a clip;
                // they must not advance its local animation clock.
                let curve_start = if target.kind == VisualTargetKindV3::VideoClip {
                    timeline.video_clips[target.index].timeline_start.seconds()
                } else {
                    start
                };
                let local = format!("T-{curve_start:.9}");
                let mut opacity = crate::backend::curve_expr(&curves.opacity, opacity, &local)?;
                if let Some((fade_start, fade_duration)) = fade {
                    opacity =
                        format!("({opacity})*clip((T-{fade_start:.9})/{fade_duration:.9},0,1)");
                }
                let px = crate::backend::curve_expr(&curves.position_x, px, &local)?;
                let py = crate::backend::curve_expr(&curves.position_y, py, &local)?;
                let sx = crate::backend::curve_expr(&curves.scale_x, sx, &local)?;
                let sy = crate::backend::curve_expr(&curves.scale_y, sy, &local)?;
                let rotation =
                    crate::backend::curve_expr(&curves.rotation_degrees, rotation, &local)?;
                let rad = format!("(({rotation})*PI/180)");
                let dx = format!("(X-W/2-({px}))");
                let dy = format!("(Y-H/2-({py}))");
                let src_x =
                    format!("W/2+(({dx})*cos({rad})+({dy})*sin({rad}))/max(({sx}),0.000001)");
                let src_y =
                    format!("H/2+(-({dx})*sin({rad})+({dy})*cos({rad}))/max(({sy}),0.000001)");
                let visible = format!(
                    "gt(({sx}),0)*gt(({sy}),0)*gte(({src_x}),0)*lt(({src_x}),W)*gte(({src_y}),0)*lt(({src_y}),H)"
                );
                // An ordinary untransformed shot must not evaluate trigonometry
                // and resample four channels at every pixel. Keep the general
                // expression path for actual geometry animation.
                let transform = if identity_geometry && constant_opacity {
                    if static_opacity == 1.0 {
                        ",format=gbrap".to_string()
                    } else {
                        format!(",format=gbrap,lut=a='val*{static_opacity:.12}'")
                    }
                } else if identity_geometry {
                    // Opacity is spatially uniform. Preserve RGB directly and
                    // evaluate only the alpha plane instead of resampling all
                    // four channels through geq for every pixel of every frame.
                    format!(
                        ",format=gbrap,split[v3rgb{layer}][v3alpha{layer}];[v3alpha{layer}]alphaextract,geq=lum='lum(X,Y)*({opacity})'[v3opacity{layer}];[v3rgb{layer}][v3opacity{layer}]alphamerge"
                    )
                } else {
                    format!(
                        ",geq=r='r({src_x},{src_y})':g='g({src_x},{src_y})':b='b({src_x},{src_y})':a='alpha({src_x},{src_y})*({opacity})*({visible})'"
                    )
                };
                // Padding fills short sources, but it must not run transforms
                // and effects past this layer's actual (handle-expanded) end.
                let end = start + duration;
                chains.push(format!("[{input}:{stream}]{source},fps={fps},tpad=stop_mode=clone:stop_duration={duration:.9},trim=end={end:.9},scale={width}:{height}:force_original_aspect_ratio=decrease,format=rgba,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2:color=black@0{transform}[{label}]"));
                (start, duration)
            }
            VisualTargetKindV3::Shape => {
                let shape = &visual.shapes[target.index];
                let cx = format!("W/2+{:.9}", shape.position_x);
                let cy = format!("H/2+{:.9}", shape.position_y);
                let rad = shape.rotation_degrees * std::f64::consts::PI / 180.0;
                let x = format!(
                    "((X-({cx}))*{:.12}+(Y-({cy}))*{:.12})",
                    rad.cos(),
                    rad.sin()
                );
                let y = format!(
                    "(-(X-({cx}))*{:.12}+(Y-({cy}))*{:.12})",
                    rad.sin(),
                    rad.cos()
                );
                let inside = match shape.primitive {
                    ShapePrimitiveV3::Rectangle {
                        width_px,
                        height_px,
                    } => format!(
                        "lt(abs({x}),{:.9})*lt(abs({y}),{:.9})",
                        width_px / 2.0,
                        height_px / 2.0
                    ),
                    ShapePrimitiveV3::Ellipse {
                        radius_x_px,
                        radius_y_px,
                    } => format!("lte((({x})/{radius_x_px:.9})^2+(({y})/{radius_y_px:.9})^2,1)"),
                };
                let alpha = f64::from(shape.fill.alpha) * shape.opacity;
                chains.push(format!("color=c=black@0:s={width}x{height}:r={fps}:d={:.9},format=rgba,geq=r='{}':g='{}':b='{}':a='{alpha:.9}*({inside})',setpts=PTS-STARTPTS+{:.9}/TB[{label}]", timeline.duration.seconds(), shape.fill.red, shape.fill.green, shape.fill.blue, shape.start.seconds()));
                (shape.start.seconds(), shape.duration.seconds())
            }
        };
        // Frame and interval requests only composite layers that can contribute
        // to that interval. Bounds include the real crossfade source handles
        // computed above; layer timing stays on the absolute sequence clock.
        let end = start + duration;
        let outside_frame = frame_time.is_some_and(|time| {
            let time = time.seconds();
            time < start || time >= end
        });
        let outside_range = range.is_some_and(|(requested_start, requested_duration)| {
            end <= requested_start.seconds()
                || start >= requested_start.seconds() + requested_duration.seconds()
        });
        if outside_frame || outside_range {
            chains.truncate(layer_chain_start);
            continue;
        }
        let mut current = label;
        for (index, effect) in visual
            .effects
            .iter()
            .filter(|effect| effect.target_id == target.target_id)
            .enumerate()
        {
            let next = format!("v3fx{layer}_{index}");
            match effect.effect {
                VisualEffectV3::ColorAdjust {
                    exposure_ev,
                    contrast,
                    saturation,
                } => {
                    let brightness = (2.0_f64.powf(exposure_ev) - 1.0).clamp(-1.0, 1.0);
                    chains.push(format!("[{current}]format=yuva444p,eq=brightness={brightness:.9}:contrast={contrast:.9}:saturation={saturation:.9},format=rgba[{next}]"));
                }
                VisualEffectV3::GaussianBlur { radius_px } => {
                    if radius_px == 0.0 {
                        chains.push(format!("[{current}]null[{next}]"));
                    } else {
                        chains.push(format!(
                            "[{current}]gblur=sigma={radius_px:.9},format=rgba[{next}]"
                        ));
                    }
                }
            }
            current = next;
        }
        for (index, mask) in visual
            .masks
            .iter()
            .filter(|mask| mask.target_id == target.target_id)
            .enumerate()
        {
            let next = format!("v3mask{layer}_{index}");
            let inside = match mask.shape {
                MaskShapeV3::Rectangle {
                    x,
                    y,
                    width,
                    height,
                } => {
                    format!(
                        "gte(X,{:.9})*lt(X,{:.9})*gte(Y,{:.9})*lt(Y,{:.9})",
                        x * f64::from(timeline.video.width),
                        (x + width) * f64::from(timeline.video.width),
                        y * f64::from(timeline.video.height),
                        (y + height) * f64::from(timeline.video.height)
                    )
                }
                MaskShapeV3::Ellipse {
                    x,
                    y,
                    width,
                    height,
                } => {
                    let cx = (x + width / 2.0) * f64::from(timeline.video.width);
                    let cy = (y + height / 2.0) * f64::from(timeline.video.height);
                    let rx = width / 2.0 * f64::from(timeline.video.width);
                    let ry = height / 2.0 * f64::from(timeline.video.height);
                    format!("lte(((X-{cx:.9})/{rx:.9})^2+((Y-{cy:.9})/{ry:.9})^2,1)")
                }
            };
            let inside = if mask.inverted {
                format!("1-({inside})")
            } else {
                inside
            };
            let inside = if mask.feather > 0.0 {
                let pixels = mask.feather * f64::from(width.min(height));
                let distance = match mask.shape {
                    MaskShapeV3::Rectangle {
                        x,
                        y,
                        width: mw,
                        height: mh,
                    } => {
                        let left = x * f64::from(width);
                        let right = (x + mw) * f64::from(width);
                        let top = y * f64::from(height);
                        let bottom = (y + mh) * f64::from(height);
                        format!("min(min(X-{left:.9},{right:.9}-X),min(Y-{top:.9},{bottom:.9}-Y))")
                    }
                    MaskShapeV3::Ellipse {
                        x,
                        y,
                        width: mw,
                        height: mh,
                    } => {
                        let cx = (x + mw / 2.0) * f64::from(width);
                        let cy = (y + mh / 2.0) * f64::from(height);
                        let rx = mw / 2.0 * f64::from(width);
                        let ry = mh / 2.0 * f64::from(height);
                        format!(
                            "(1-sqrt(((X-{cx:.9})/{rx:.9})^2+((Y-{cy:.9})/{ry:.9})^2))*{:.9}",
                            rx.min(ry)
                        )
                    }
                };
                let feather_alpha = format!("clip(0.5+({distance})/{pixels:.9},0,1)");
                if mask.inverted {
                    format!("1-({feather_alpha})")
                } else {
                    feather_alpha
                }
            } else {
                inside
            };
            chains.push(format!("[{current}]format=rgba,geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='alpha(X,Y)*({inside})'[{next}]"));
            current = next;
        }
        let next = format!("v3base{}", layer + 1);
        chains.push(format!("[{base}][{current}]overlay=x=0:y=0:eof_action=pass:enable='gte(t,{start:.9})*lt(t,{:.9})'[{next}]", start+duration));
        base = next;
    }
    // V3's RGB graphics and SDR media compositor has a defined delivery space.
    // Convert the actual samples as well as carrying the frame metadata: merely
    // tagging an unspecified YUV matrix as BT.709 changes decoded colors.
    chains.push(format!(
        "[{base}]format=gbrp,scale=out_color_matrix=bt709:out_range=tv,format=yuv420p,setparams=range=limited:color_primaries=bt709:color_trc=bt709:colorspace=bt709[v3sdr]"
    ));
    // The base is already on the requested frame. finish_timeline_filter's
    // frame selection counts frames, whereas all upstream PTS remain absolute.
    let selected_frame = frame_time.map(|_| MediaTime {
        ticks: 0,
        timebase: 1,
    });
    crate::backend::finish_timeline_filter(timeline, selected_frame, range, chains, "v3sdr".into())
}

fn validate(timeline: &TimelineExportRequest, graph: &CompiledVisualGraphV3) -> MediaResult<()> {
    if graph.targets.len() > 10_000
        || graph.effects.len() > 10_000
        || graph.masks.len() > 10_000
        || graph.shapes.len() > 10_000
    {
        return Err(MediaError::InvalidRequest(
            "visual graph is too large".into(),
        ));
    }
    let mut ids = HashSet::new();
    for target in &graph.targets {
        if target.target_id.is_empty() || !ids.insert(&target.target_id) {
            return Err(MediaError::InvalidRequest(
                "visual targets must have unique IDs".into(),
            ));
        }
        let count = match target.kind {
            VisualTargetKindV3::VideoClip => timeline.video_clips.len(),
            VisualTargetKindV3::ImageClip => timeline.image_clips.len(),
            VisualTargetKindV3::Shape => graph.shapes.len(),
        };
        if target.index >= count {
            return Err(MediaError::InvalidRequest(
                "visual target index is out of range".into(),
            ));
        }
    }
    for shape in &graph.shapes {
        if !shape.position_x.is_finite()
            || !shape.position_y.is_finite()
            || !shape.rotation_degrees.is_finite()
            || !shape.opacity.is_finite()
        {
            return Err(MediaError::InvalidRequest(
                "shape parameters must be finite".into(),
            ));
        }
        let size_valid = match shape.primitive {
            ShapePrimitiveV3::Rectangle {
                width_px,
                height_px,
            } => width_px.is_finite() && height_px.is_finite() && width_px > 0.0 && height_px > 0.0,
            ShapePrimitiveV3::Ellipse {
                radius_x_px,
                radius_y_px,
            } => {
                radius_x_px.is_finite()
                    && radius_y_px.is_finite()
                    && radius_x_px > 0.0
                    && radius_y_px > 0.0
            }
        };
        if !size_valid
            || !(0.0..=1.0).contains(&shape.opacity)
            || shape.duration.ticks <= 0
            || shape.duration.timebase == 0
            || shape.start.ticks < 0
            || shape.start.timebase == 0
        {
            return Err(MediaError::InvalidRequest(
                "shape geometry or timing is invalid".into(),
            ));
        }
    }
    for effect in &graph.effects {
        if !ids.contains(&effect.target_id) {
            return Err(MediaError::InvalidRequest(
                "effect target is missing".into(),
            ));
        }
        let valid = match effect.effect {
            VisualEffectV3::ColorAdjust {
                exposure_ev,
                contrast,
                saturation,
            } => {
                exposure_ev.is_finite()
                    && contrast.is_finite()
                    && saturation.is_finite()
                    && (-8.0..=8.0).contains(&exposure_ev)
                    && (0.0..=4.0).contains(&contrast)
                    && (0.0..=4.0).contains(&saturation)
            }
            VisualEffectV3::GaussianBlur { radius_px } => {
                radius_px.is_finite() && (0.0..=128.0).contains(&radius_px)
            }
        };
        if !valid {
            return Err(MediaError::InvalidRequest(
                "effect parameters are invalid".into(),
            ));
        }
    }
    for mask in &graph.masks {
        if !ids.contains(&mask.target_id) {
            return Err(MediaError::InvalidRequest("mask target is missing".into()));
        }
        let (x, y, width, height) = match mask.shape {
            MaskShapeV3::Rectangle {
                x,
                y,
                width,
                height,
            }
            | MaskShapeV3::Ellipse {
                x,
                y,
                width,
                height,
            } => (x, y, width, height),
        };
        if ![x, y, width, height, mask.feather]
            .iter()
            .all(|value| value.is_finite())
            || x < 0.0
            || y < 0.0
            || width <= 0.0
            || height <= 0.0
            || x + width > 1.0
            || y + height > 1.0
            || mask.feather < 0.0
        {
            return Err(MediaError::InvalidRequest(
                "mask geometry is invalid".into(),
            ));
        }
    }
    transition_source_handles(timeline, graph)?;
    Ok(())
}
