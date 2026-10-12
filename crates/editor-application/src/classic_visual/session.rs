use super::*;
use opencut_composition::project_v4::*;
use opencut_contracts::AssetKind;
use opencut_job_execution::project_v4_frame::LocalVisualExecutionPolicyV1;
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub struct ClassicVisualSession<'a> {
    prepared: &'a PreparedVisualFrameV4,
    policy: LocalVisualExecutionPolicyV1,
    admissions: BTreeSet<String>,
    bound: Option<BoundVisualFrameV4>,
    received_bytes: u64,
    rgba_bytes: u64,
    decoded_limit_pixels: u64,
}
fn budget(message: &str) -> EditorError {
    EditorError::new("VISUAL_BUDGET_EXCEEDED", message)
}
impl<'a> ClassicVisualSession<'a> {
    pub fn new(
        prepared: &'a PreparedVisualFrameV4,
        policy: &LocalVisualExecutionPolicyV1,
    ) -> Result<Self, EditorError> {
        crate::project_v4_frame_jobs::validate_policy(policy)?;
        let i = prepared.identity();
        let pixels = u64::from(i.width)
            .checked_mul(u64::from(i.height))
            .ok_or_else(|| budget("Canvas pixel overflow"))?;
        let rgba_bytes = pixels
            .checked_mul(4)
            .ok_or_else(|| budget("RGBA overflow"))?;
        let padded_row = (u64::from(i.width) * 4).div_ceil(256) * 256;
        let readback = padded_row
            .checked_mul(u64::from(i.height))
            .ok_or_else(|| budget("Readback overflow"))?;
        // Eight full canvas textures cover scene/layer/blend/background/readback
        // staging work. Decode reserves are additional, never borrowed from it.
        let gpu_base = rgba_bytes
            .checked_mul(8)
            .and_then(|n| n.checked_add(readback))
            .ok_or_else(|| budget("GPU overflow"))?;
        let mut encoded = 0u64;
        for resource in prepared.resources() {
            if resource.version.size == 0 || resource.version.size > policy.max_encoded_bytes {
                return Err(budget("Encoded resource exceeds admission"));
            }
            encoded = encoded
                .checked_add(resource.version.size)
                .ok_or_else(|| budget("Encoded overflow"))?;
        }
        let encoded_reserve = encoded
            .checked_mul(3)
            .ok_or_else(|| budget("Encoded reserve overflow"))?;
        let gpu_remaining = policy
            .max_gpu_bytes
            .checked_sub(gpu_base)
            .ok_or_else(|| budget("Frame GPU budget exceeded"))?;
        let rgba_remaining = policy
            .max_rgba_bytes
            .checked_sub(rgba_bytes)
            .and_then(|n| n.checked_sub(encoded_reserve))
            .ok_or_else(|| budget("Frame RGBA budget exceeded"))?;
        let decoded_limit_pixels = policy
            .max_decoded_pixels
            .min(gpu_remaining / 16)
            .min(rgba_remaining / 16);
        if pixels == 0
            || pixels > policy.max_frame_pixels
            || prepared.layers().len() > 60
            || prepared.resources().len() > 32
            || (decoded_limit_pixels == 0 && !prepared.layers().is_empty())
        {
            return Err(budget("Visual frame exceeds pixel/layer admission"));
        }
        Ok(Self {
            prepared,
            policy: policy.clone(),
            admissions: BTreeSet::new(),
            bound: None,
            received_bytes: 0,
            rgba_bytes,
            decoded_limit_pixels,
        })
    }
    pub fn prepared(&self) -> &PreparedVisualFrameV4 {
        self.prepared
    }
    pub fn policy(&self) -> &LocalVisualExecutionPolicyV1 {
        &self.policy
    }
    pub fn resource(&self, id: &str) -> Result<VisualResource, EditorError> {
        let r = self
            .prepared
            .resources()
            .iter()
            .find(|r| r.asset_id == id)
            .ok_or_else(|| invalid("Unknown visual resource"))?;
        let kind = match r.kind {
            AssetKind::Video => VisualResourceKind::Video,
            AssetKind::Image | AssetKind::Graphic => VisualResourceKind::Image,
            _ => return Err(invalid("Unsupported visual resource kind")),
        };
        Ok(VisualResource {
            asset_id: id.into(),
            content_hash: r.version.content_hash.clone(),
            size: r.version.size.to_string(),
            kind,
        })
    }
    pub fn typed_plan(&self) -> Result<ClassicVisualPlan, EditorError> {
        let mut layers = Vec::new();
        for layer in self.prepared.layers() {
            let (id, decode) = match &layer.content {
                SampledContentV4::Image { asset_id } => (asset_id, VisualDecodeRequest::Image),
                SampledContentV4::Video {
                    asset_id,
                    stream,
                    source,
                    ..
                } => (
                    asset_id,
                    VisualDecodeRequest::Video {
                        requested_time: source.source_time.clone(),
                        requested_time_seconds: {
                            let t = opencut_time::RationalTime::try_from(&source.source_time)?;
                            t.ticks() as f64 / f64::from(t.timebase())
                        },
                        point_profile: "classic_video_rounded_ticks".into(),
                        selected_video_stream: *stream,
                        no_frame_reason: if source.sample == SourceReadStateV4::InBounds {
                            None
                        } else {
                            Some(VisualNoFrameReasonV4::OutsideReadWindow)
                        },
                    },
                ),
                _ => return Err(invalid("Preparation contains nonmedia visual node")),
            };
            layers.push(ClassicVisualLayerPlan {
                element_id: layer.element_id.clone(),
                resource: self.resource(id)?,
                decode,
            });
        }
        Ok(ClassicVisualPlan {
            identity: self.prepared.identity().clone(),
            resources: self
                .prepared
                .resources()
                .iter()
                .map(|r| self.resource(&r.asset_id))
                .collect::<Result<_, _>>()?,
            layers,
        })
    }
    pub fn plan(&self) -> Result<Value, EditorError> {
        serde_json::to_value(self.typed_plan()?)
            .map_err(|_| invalid("Cannot serialize visual plan"))
    }
    pub fn bound(&self) -> Option<&BoundVisualFrameV4> {
        self.bound.as_ref()
    }
    /// Pure validation also used when projecting a persisted completion result.
    pub fn project_decoded_sources(
        &self,
        decoded: &[DecodedVisualSourceV4],
    ) -> Result<BoundVisualFrameV4, EditorError> {
        let per_layer = self.decoded_limit_pixels / (self.prepared.layers().len() as u64).max(1);
        for observation in decoded {
            if let DecodedVisualSourceV4::ReadyImage { width, height, .. }
            | DecodedVisualSourceV4::ReadyVideo { width, height, .. } = observation
                && (*width > 16384
                    || *height > 16384
                    || u64::from(*width) * u64::from(*height) > per_layer)
            {
                return Err(budget("Decoded dimensions exceed prior admission"));
            }
        }
        bind_visual_sources_v4(self.prepared, decoded).map_err(|e| invalid(e.message))
    }
    pub fn callback(
        &mut self,
        request: VisualCallback,
        resources: &mut (impl ClassicVisualResources + ?Sized),
    ) -> Result<Value, EditorError> {
        resources.checkpoint()?;
        match request {
            VisualCallback::Resolve(v) => {
                if self.resource(&v.resource.asset_id)? != v.resource {
                    return Err(invalid("Resource differs from fixed revision"));
                }
                let (id, size) = resources.resolve(&v.resource, &v.handle)?;
                if size != decimal(&v.resource.size)? {
                    return Err(invalid("Verified resource size changed"));
                }
                Ok(json!({"transferId":id,"byteLength":size.to_string()}))
            }
            VisualCallback::Read(v) => {
                if v.count == 0 || v.count > 65536 {
                    return Err(invalid("Invalid visual byte chunk"));
                }
                let (bytes, eof) = resources.read(&v.transfer_id, decimal(&v.offset)?, v.count)?;
                if bytes.len() > v.count as usize {
                    return Err(invalid("Resource port exceeded chunk"));
                }
                let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
                Ok(json!({"offset":v.offset,"bytesHex":hex,"eof":eof}))
            }
            VisualCallback::Release(v) => {
                resources.release(&v.transfer_id)?;
                Ok(json!({"released":true}))
            }
            VisualCallback::Admit(v) => {
                if self.bound.is_some()
                    || !self
                        .prepared
                        .layers()
                        .iter()
                        .any(|l| l.element_id == v.element_id)
                    || !self.admissions.insert(v.element_id)
                {
                    return Err(invalid(
                        "Decode admission is unknown, repeated or already bound",
                    ));
                }
                let max_pixels =
                    self.decoded_limit_pixels / (self.prepared.layers().len() as u64).max(1);
                if max_pixels == 0 {
                    return Err(budget("No decoded pixels available"));
                }
                let admission = VisualDecodeAdmission {
                    max_width: 16384,
                    max_height: 16384,
                    max_pixels: max_pixels.to_string(),
                    max_decoded_bytes: (max_pixels * 16).to_string(),
                };
                serde_json::to_value(admission)
                    .map_err(|_| invalid("Cannot serialize visual admission"))
            }
            VisualCallback::Bind(v) => {
                if self.bound.is_some() || self.admissions.len() != self.prepared.layers().len() {
                    return Err(invalid(
                        "Frame must bind once after complete decode admission",
                    ));
                }
                let bound = self.project_decoded_sources(&v.decoded)?;
                let value = serde_json::to_value(&bound)
                    .map_err(|_| invalid("Cannot serialize bound frame"))?;
                self.bound = Some(bound);
                Ok(value)
            }
        }
    }
    pub fn rgba_chunk(
        &mut self,
        offset: u64,
        bytes: &[u8],
        sink: &mut (impl ClassicVisualRgbaSink + ?Sized),
    ) -> Result<(), EditorError> {
        if self.bound.is_none()
            || offset != self.received_bytes
            || bytes.is_empty()
            || bytes.len() > 65536
            || !bytes.len().is_multiple_of(4)
        {
            return Err(invalid(
                "RGBA chunks must follow bound frame in sequential pixel order",
            ));
        }
        let end = offset
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| invalid("RGBA offset overflow"))?;
        if end > self.rgba_bytes {
            return Err(invalid("RGBA chunk exceeds frame"));
        }
        sink.chunk(offset, bytes)?;
        self.received_bytes = end;
        Ok(())
    }
    pub fn validate_receipt(
        &self,
        receipt: &ClassicVisualRenderReceipt,
    ) -> Result<(), EditorError> {
        if receipt.identity != *self.prepared.identity()
            || self.bound.as_ref() != Some(&receipt.bound)
            || self.received_bytes != self.rgba_bytes
            || decimal(&receipt.rgba_bytes)? != self.rgba_bytes
            || decimal(&receipt.peak_accounted_bytes)? > self.policy.max_rgba_bytes
            || receipt.engine_identity != CLASSIC_VISUAL_PROFILE_V4
        {
            return Err(invalid(
                "Receipt differs from complete fixed frame execution",
            ));
        }
        Ok(())
    }
}
