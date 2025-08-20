Report

Issue:
test_race_car_with_netns_and_video_output receives thousands of RTP packets but never records a video frame.
Runtime logs show tsdemux only exposing an AAC audio pad; no H.265 video pad appears. Consequently, progress reports remain at “Received: 0 frames, 259 KB” throughout the run.

Likely Cause:
In the sender pipeline, the H.265 stream is parsed without any configuration before being fed to mpegtsmux. Because h265parse is left at its defaults, mpegtsmux never advertises the video elementary stream in the TS Program Map Table, so tsdemux on the receiver side only detects audio
.
The receiver pipeline, by contrast, explicitly configures h265parse to output an MP4‑friendly stream (config-interval, stream-format, and alignment)

.

Recommended Fixes:

    Configure H.265 parser on the sender side

    let vparse = gstreamer::ElementFactory::make("h265parse")
        .property("config-interval", 1i32)
        .build()?;

    Ensures the parser outputs codec headers frequently so mpegtsmux can register the video stream.

    Validate MPEG‑TS contents

        Inspect sender_debug.ts with gst-discoverer-1.0 or ffprobe to confirm both video and audio PIDs are present.

        If the video track is missing, verify that mpegtsmux supports H.265 (requires a recent GStreamer “bad” plugin set).

    Double‑check RTP payload mapping

        Explicitly set payload-type=33 on both ristsink and ristsrc to avoid PT mismatches.

    Regression test

        After applying the fixes, rerun the 60‑second test and confirm frames_received increments and tsdemux announces a video pad.

Implementing these changes should allow the receiver to decode the video stream, letting the test pass and completing the project.