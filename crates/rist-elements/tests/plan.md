Testing Gaps & Recommended Additions
Dispatcher Element

AIMD strategy lacks behavioral verification – only property access is tested; weight adaptation under AIMD is unverified.
Suggested taskTest AIMD adaptation in dispatcher

Keyframe duplication logic remains untested – duplicate_keyframes and dup_budget_pps properties are not covered.
Suggested taskVerify keyframe duplication budget handling

Metrics export functionality is missing test coverage – metrics_export_interval_ms should emit bus messages.
Suggested taskTest dispatcher metrics bus messages

Pad removal during streaming is untested – need to ensure cached events and state cleanly handle release_request_pad.
Suggested taskStress test dispatcher pad removal while playing

Invalid or malformed weight inputs not validated – no tests for bad JSON or negative weights.
Suggested taskValidate weight parsing error paths
Dynamic Bitrate Element

Downscale keyframe triggering is not exercised – downscale-keyunit property has no test.
Suggested taskTest keyframe forcing on bitrate drop

Dispatcher coordination not validated – setting dispatcher should disable its auto-balance.
Suggested taskEnsure dynbitrate disables dispatcher auto-balance

Encoder bitrate property detection and scaling lack tests – need coverage for non‑standard property names/units.
Suggested taskCover encoder bitrate property detection

Behavior with missing encoder or RIST elements untested – tick should handle None gracefully.
Suggested taskHandle unset dependencies gracefully

Multiple-session statistics and aggregate parsing not covered – only single-session stats are exercised.
Suggested taskTest multi-session stats parsing

Timer cleanup on dispose is unverified – ensure periodic tick source is removed to avoid leaks.
Suggested taskCheck dynbitrate tick source removal

By addressing the above gaps, the dispatcher and dynamic bitrate elements will have far more comprehensive coverage, improving confidence in their robustness.