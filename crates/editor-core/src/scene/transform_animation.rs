//! Transform-channel selector shared by fixed-revision draft admission. Other
//! animation targets never implicitly disable a static element transform.
use super::*;
pub fn channel_affects_element_transform(channel: &AnimationChannel, element_id: &str) -> bool {
    matches!(channel,AnimationChannel::Scalar{
        target:ChannelTarget::Element{element_id:id},
        binding:ScalarBinding::Property{parameter:ScalarParameter::PositionXPx|ScalarParameter::PositionYPx|ScalarParameter::ScaleX|ScalarParameter::ScaleY|ScalarParameter::RotationDegrees},..
    } if id==element_id)
}
pub fn has_element_transform_animation(element: &TimelineElement) -> bool {
    element
        .channels
        .iter()
        .any(|channel| channel_affects_element_transform(channel, &element.id))
}
