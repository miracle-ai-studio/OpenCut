#!/usr/bin/env python3
"""Real five-minute VFR source -> split/retime -> late interval, in retained CAS.

The known source is generated once in the owned workspace, never in the repo.
Its alternating colors/frequencies are authored independent expectations.
"""
import argparse
import json
import math
import subprocess
from array import array
from fractions import Fraction
from pathlib import Path
from acceptance import Harness, at, seconds, sha


def main(args):
    h = Harness(args.binary, args.workspace, "agent-long-v4")
    source = h.workspace / "agent-long-vfr-300s.mp4"
    assert source.is_file(), "Prepare the one owned 300s VFR/44.1k stereo source before this run"
    project_id, sequence_id = "agent-long-v4-acceptance", "long-scene"

    def profile():
        probe = json.loads(subprocess.check_output(["ffprobe", "-v", "error", "-show_format", "-show_streams", "-of", "json", str(source)]))
        video = next(s for s in probe["streams"] if s["codec_type"] == "video")
        audio = next(s for s in probe["streams"] if s["codec_type"] == "audio")
        assert video["width"] == 1920 and video["height"] == 1080
        assert audio["sample_rate"] == "44100" and audio["channels"] == 2
        assert float(probe["format"]["duration"]) == 300
        packets = json.loads(subprocess.check_output(["ffprobe", "-v", "error", "-select_streams", "v:0",
            "-show_packets", "-show_entries", "packet=pts_time", "-of", "json", str(source)]))["packets"]
        timestamps = [Fraction(p["pts_time"]) for p in packets]
        deltas = sorted({b - a for a, b in zip(timestamps, timestamps[1:])})
        assert len(timestamps) == 7200 and max(deltas) > min(deltas) * Fraction(3, 2)
        return {"sha256": sha(source.read_bytes()), "size": source.stat().st_size, "duration_seconds": 300,
            "width": 1920, "height": 1080, "video_packets": len(timestamps), "packet_deltas": [str(n) for n in deltas],
            "audio_sample_rate": 44100, "audio_channels": 2,
            "expected": "red/even seconds, lime/odd; stereo 0.08 sine at 440/660 Hz on even, 880/1320 Hz on odd; 30fps input with n%5==1 dropped, VFR timestamps retained"}
    source_profile = h.stage("source-profile", profile)
    assert sha(source.read_bytes()) == source_profile["sha256"]
    created = h.stage("create", lambda: h.request("create", "opencut_project_create", {
        "project": {"id": project_id, "name": "Five-minute source VFR acceptance", "sequence_id": sequence_id,
            "width": 320, "height": 180, "fps": {"numerator": 30, "denominator": 1}}, "idempotency_key": h.key("create")}))
    media = h.stage("import-long-source", lambda: h.import_media("long-source", project_id, source))
    assert media["asset"]["version"]["content_hash"] == source_profile["sha256"]
    def place():
        snapshot = h.inspect(project_id)
        scene = snapshot["project"]["sequences"][0]
        plan = h.request("place-plan", "opencut_media_import_plan", {
            "project_id": project_id, "expected_revision": snapshot["revision"], "sequence_id": sequence_id,
            "idempotency_key": h.key("place"), "label": "Place final six seconds of original long source", "ripple": False,
            "bindings": [{"media_id": media["media_id"], "version": media["asset"]["version"], "asset_id": "long-source",
                "name": source.name, "mode": "bind"}], "new_tracks": [], "placements": [{
                "element_id": "long-left", "name": "Late long source", "track_id": scene["main_track_id"], "asset_id": "long-source",
                "start": at(0), "content": {"kind": "video", "source_audio_enabled": True,
                    "timing": {"kind": "source_range", "range": {"start": at(294), "duration": at(6)},
                        "rate": {"numerator": 1, "denominator": 1}, "maintain_pitch": False}},
            }]})
        h.apply("place", plan["transaction"])
        current = h.inspect(project_id)
        settings = current["project"]["settings"]
        assert settings["profile"] == "classic_global"
        settings["settings"].update(fps={"numerator": 30, "denominator": 1}, canvas_size={"width": 320, "height": 180},
            canvas_size_mode="custom", last_custom_canvas_size={"width": 320, "height": 180})
        result = h.apply("settings", {"project_id": project_id, "expected_revision": current["revision"],
            "idempotency_key": h.key("settings"), "label": "Fixed 30fps observation grid",
            "operations": [{"type": "settings_set", "settings": settings}]})
        return {"revision": result["revision"], "track_id": scene["main_track_id"]}
    placed = h.stage("place", place)
    def split_retime():
        saved_split = h.state["requests"].get("split")
        plan = {"transaction": saved_split["input"]} if saved_split else h.request("split-plan", "opencut_edit_agent_plan", {
            "project_id": project_id, "expected_revision": placed["revision"], "idempotency_key": h.key("split"),
            "label": "Split original source at 296s", "steps": [{"kind": "timeline", "sequence_id": sequence_id,
                "command": {"kind": "split", "targets": [{"target": {"track_id": placed["track_id"], "element_id": "long-left"},
                    "right_element_id": "long-right"}], "at": at(2), "retain": "both", "ripple": False}}]})
        receipt = h.apply("split", plan["transaction"])
        snapshot = h.inspect(project_id)
        right = snapshot["project"]["sequences"][0]["tracks"][0]["elements"][1]
        retime = right["content"]["retime"]
        retime.update(rate={"numerator": 5, "denominator": 4}, maintain_pitch=False)
        retimed = h.request("retime-plan", "opencut_element_properties_plan", {"project_id": project_id,
            "expected_revision": receipt["revision"], "sequence_id": sequence_id, "element_id": "long-right",
            "idempotency_key": h.key("retime"), "label": "Explicit 1.25x without pitch preservation",
            "transform": None, "opacity": None, "audio_gain": None, "muted": None, "retime": retime})
        committed = h.apply("retime", retimed)
        return {"revision": committed["revision"]}
    retimed = h.stage("split-and-retime", split_retime)
    scope = {"project_id": project_id, "revision": retimed["revision"], "sequence_id": sequence_id}
    def source_evidence():
        result = h.request("late-evidence", "opencut_source_art_evidence", {"project_id": project_id,
            "revision": scope["revision"], "request_id": h.key("late-evidence"),
            "frames": [{"asset_id": "long-source", "time": at(299.25)}],
            "waveforms": [{"asset_id": "long-source", "range": {"start": at(299), "duration": at(.5)}, "bins": 64, "sample_rate": 48000}]})
        assert seconds(result["receipts"][0]["requested_time"]) == Fraction(1197, 4)
        image = bytes.fromhex(result["contact_sheet"]["bytes_hex"])
        assert sha(image) == result["contact_sheet"]["content_hash"]
        (h.workspace / "agent-long-source-evidence.svg").write_bytes(image)
        return {"content_hash": sha(image), "receipts": result["receipts"]}
    h.stage("late-source-frame-and-waveform", source_evidence)
    mapped = h.stage("source-clock-mapping", lambda: h.call("opencut_media_map_time", {**scope,
        "target": {"track_id": placed["track_id"], "element_id": "long-right"}, "direction": "sequence_to_source",
        "times": [at(4.2), at(4.4), at(5.2)]}))
    assert [seconds(p["mapped"]) for p in mapped["points"]] == [Fraction(1195, 4), Fraction(299), Fraction(300)]
    assert args.native_runtime, "Capture current audio/visual module identities before output"
    h.runtime = json.loads(args.native_runtime.read_text())
    episode = h.candidate[:12] + "-" + sha(json.dumps(h.runtime, sort_keys=True).encode())[:12]
    output_stage = "late-range-export-" + episode
    output = h.stage(output_stage, lambda: h.job(output_stage, "opencut_render_submit", {
        "kind": "video_range", "request": {**scope, "idempotency_key": h.key(output_stage),
            "options": {"format": "mp4", "quality": "high", "include_audio": True},
            "range": {"start_frame": "126", "frame_count": "30"}}}))
    assert output["extended_result"]["data"]["output"]["execution"]["audio"]["module_sha256"] == h.runtime["audio_module_sha256"]
    assert output["extended_result"]["data"]["output"]["execution"]["visual"]["module_sha256"] == h.runtime["visual_module_sha256"]
    assert args.legacy_reference, "Capture the original whole-source decoder reference first"
    def compare_legacy():
        reference = json.loads(args.legacy_reference.read_text())
        expected_bytes = Path(reference["pcm_path"]).read_bytes()
        assert reference["source_sha256"] == source_profile["sha256"]
        assert sha(expected_bytes) == reference["pcm_sha256"]
        assert reference["role"] == "oracle_only_not_product_policy_pass"
        execution = output["extended_result"]["data"]["output"]["execution"]["audio"]
        assert execution["browser_sha256"] == reference["runtime"]["browserSha256"]
        actual = array("f")
        actual.frombytes(subprocess.check_output(["ffmpeg", "-v", "error", "-i", str(h.artifact(output).parent / "video-audio.wav"),
            "-f", "f32le", "-ac", "2", "-ar", "48000", "pipe:1"]))
        expected = array("f"); expected.frombytes(expected_bytes)
        assert len(actual) == len(expected) == 96000
        errors = [abs(a - b) for a, b in zip(actual, expected)]
        maximum = max(errors)
        assert maximum <= 1e-5, {"legacy_whole_pcm_max_error": maximum}
        return {"reference_report": str(args.legacy_reference), "reference_pcm_sha256": reference["pcm_sha256"],
            "max_absolute_error": maximum, "rms_error": math.sqrt(sum(e * e for e in errors) / len(errors)),
            "threshold": 1e-5, "frames": 48000, "channels": 2,
            "scope": "Normal product policy output against unchanged whole-source decoder; larger oracle admission is not product evidence"}
    legacy = h.stage("legacy-whole-pcm-" + output["job_id"], compare_legacy)
    def independent():
        path = h.artifact(output)
        assert args.color_metadata, "Observe original decoded source color metadata, not output colors"
        color = json.loads(args.color_metadata.read_text())
        assert color["source_sha256"] == source_profile["sha256"]
        assert color["runtime"]["browserSha256"] == output["extended_result"]["data"]["output"]["execution"]["visual"]["browser_sha256"]
        assert len(color["samples"]) == 2
        assert all(row["color_space"] == {"fullRange": False, "matrix": "bt709", "primaries": "bt709", "transfer": "bt709"} for row in color["samples"])
        # The original fixture has untagged limited-range YUV, not encoded RGB.
        # Check its independently authored source components, then apply the
        # observed source frame's Rec709 matrix. Never derive expected pixels
        # from the exported image or relax the original channel threshold.
        source_yuv = [(81, 90, 240), (144, 54, 34)]
        for time, (y, u, v) in zip(["298.75", "299.25"], source_yuv):
            raw = subprocess.check_output(["ffmpeg", "-v", "error", "-ss", time, "-i", str(source), "-frames:v", "1",
                "-vf", "crop=2:2:960:540", "-pix_fmt", "yuv420p", "-f", "rawvideo", "pipe:1"])
            assert raw == bytes([y, y, y, y, u, v])
        def rec709(yuv):
            y, u, v = yuv; luma = (y - 16) / 219; cb = (u - 128) / 224; cr = (v - 128) / 224
            r, b = luma + 1.5748 * cr, luma + 1.8556 * cb
            g = (luma - .2126 * r - .0722 * b) / .7152
            return tuple(round(max(0, min(1, n)) * 255) for n in (r, g, b))
        expected_colors = [rec709(yuv) for yuv in source_yuv]
        rgb = subprocess.check_output(["ffmpeg", "-v", "error", "-i", str(path), "-an", "-pix_fmt", "rgb24", "-f", "rawvideo", "pipe:1"])
        assert len(rgb) == 30 * 320 * 180 * 3
        classes, worst = [], 0
        for index in range(30):
            sample = rgb[(index * 320 * 180 + 90 * 320 + 160) * 3:][:3]
            green = index >= 6
            expected = expected_colors[int(green)]
            worst = max(worst, *(abs(a - b) for a, b in zip(sample, expected)))
            classes.append(sample[1] > sample[0])
        assert worst <= 8, {"pixel_error": worst, "classes": classes}
        pcm = array("f")
        pcm.frombytes(subprocess.check_output(["ffmpeg", "-v", "error", "-i", str(path.parent / "video-audio.wav"),
            "-f", "f32le", "-ac", "2", "-ar", "48000", "pipe:1"]))
        assert len(pcm) == 96000
        def power(channel, center, frequency):
            start = round(center * 48000) - 480
            values = pcm[channel::2][start:start + 960]
            return abs(sum(v * complex(math.cos(2 * math.pi * frequency * i / 48000), math.sin(2 * math.pi * frequency * i / 48000)) for i, v in enumerate(values)))
        for channel, low, high in [(0, 550, 1100), (1, 825, 1650)]:
            assert power(channel, .1, low) > 8 * power(channel, .1, high)
            assert power(channel, .4, high) > 8 * power(channel, .4, low)
        crossings = [i / 200 for i in range(20, 61) if power(0, i / 200, 1100) > power(0, i / 200, 550)]
        assert crossings
        audio_switch = min(crossings)
        video_switch = next(i for i, green in enumerate(classes) if green) / 30
        assert abs(audio_switch - video_switch) <= 1 / 30
        return {"frames": 30, "max_channel_error": worst, "channel_threshold": 8, "source_yuv": source_yuv,
            "expected_rgb": expected_colors, "source_color_metadata": str(args.color_metadata),
            "color_reference": "https://www.w3.org/TR/2026/WD-webcodecs-20261007/#videocolorspace",
            "pcm_frames": 48000, "sample_rate": 48000,
            "channel_frequencies_hz": [[550, 1100], [825, 1650]], "audio_switch_seconds": audio_switch,
            "video_switch_seconds": video_switch, "av_event_difference_seconds": abs(audio_switch - video_switch)}
    oracle = h.stage("independent-original-source-clock-av-" + output["job_id"], independent)
    quality = h.stage("quality-" + output["job_id"], lambda: h.call("opencut_quality_inspect", {
        "artifact_id": output["artifacts"][0]["artifact_id"], "caption_frame_job_ids": []}))
    assert quality["measurements"]["video_frames_decoded"] == "30"
    assert quality["technical_findings"] == ["frozen_intervals_detected"], quality["technical_findings"]
    assert quality["measurements"]["frozen_intervals"] == [{"start_seconds": .2, "end_seconds": 1.0}]
    assert quality["measurements"]["black_intervals"] == []
    assert quality["timestamp_av_offset_seconds"] == 0
    report = {"status": "passed_scoped", "whole_migration_complete": False, "binary_sha256": h.candidate,
        "workspace": str(h.workspace), "project_id": project_id, "revision": scope["revision"],
        "source": source_profile, "output_job_id": output["job_id"], "native_runtime": h.runtime, "legacy_pcm": legacy, "independent": oracle,
        "quality": {"video_frames_decoded": "30", "timestamp_av_offset_seconds": 0,
            "technical_findings": quality["technical_findings"], "expected_static_interval": quality["measurements"]["frozen_intervals"],
            "scope": "Intentional constant-green source interval is detected as frozen, not a dropped frame or a zero-finding claim"},
        "stages": {name: record["candidate"] for name, record in h.state["stages"].items()},
        "limits": ["Known synthetic five-minute 1080p VFR source; six-second authored selection and one-second encoded interval",
            "Explicit 1.25x without pitch preservation, 44.1k stereo input; not a complete codec/length/pitch/browser matrix"]}
    args.report.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--binary", type=Path, required=True)
    p.add_argument("--workspace", type=Path, required=True)
    p.add_argument("--report", type=Path, required=True)
    p.add_argument("--native-runtime", type=Path, help="Captured current audio/visual module identities")
    p.add_argument("--legacy-reference", type=Path, help="Original whole-source decoder PCM oracle report")
    p.add_argument("--color-metadata", type=Path, help="Observed original VideoFrame color metadata; never output-derived RGB")
    main(p.parse_args())
