//! Transform-channel selector shared by fixed-revision draft admission. Other
//! animation targets never implicitly disable a static element transform.
use super::*;
pub fn channel_affects_element_transform_v1(
    channel: &AnimationChannelV4,
    element_id: &str,
) -> bool {
    matches!(channel,AnimationChannelV4::Scalar{
        target:ChannelTargetV4::Element{element_id:id},
        binding:ScalarBindingV4::Property{parameter:ScalarParameterV4::PositionXPx|ScalarParameterV4::PositionYPx|ScalarParameterV4::ScaleX|ScalarParameterV4::ScaleY|ScalarParameterV4::RotationDegrees},..
    } if id==element_id)
}
pub fn has_element_transform_animation_v1(element: &TimelineElementV4) -> bool {
    element
        .channels
        .iter()
        .any(|channel| channel_affects_element_transform_v1(channel, &element.id))
}
