//! Explicit read-only visual edit admission, independent of native resources.
use crate::element_transform::TransformGestureIntent;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VisualProperty {
    PositionXPx,
    PositionYPx,
    ScaleX,
    ScaleY,
    RotationDegrees,
    Opacity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VisualEditDraftIntent {
    Gesture { gesture: TransformGestureIntent },
    Property { property: VisualProperty },
}
