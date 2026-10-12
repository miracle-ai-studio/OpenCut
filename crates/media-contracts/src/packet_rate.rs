//! Versioned Classic packet-prefix observation carried in reserved asset metadata.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const CLASSIC_PACKET_RATE_METADATA_KEY_V1: &str = "classic_packet_rate_v1";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassicPacketRateV1 {
    pub schema_version: u32,
    pub packet_count: u64,
    pub average_packet_rate: Option<f64>,
}

impl ClassicPacketRateV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("Unsupported Classic packet-rate observation version".into());
        }
        if let Some(rate) = self.average_packet_rate
            && (self.packet_count == 0 || !rate.is_finite() || rate <= 0.0)
        {
            return Err(
                "Known Classic packet rate requires packets and a finite positive rate".into(),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_wrong_version_and_invalid_known_observations() {
        for (version, count, rate) in [
            (2, 100, 30.0),
            (1, 0, 30.0),
            (1, 100, 0.0),
            (1, 100, -1.0),
            (1, 100, f64::NAN),
            (1, 100, f64::INFINITY),
        ] {
            assert!(
                ClassicPacketRateV1 {
                    schema_version: version,
                    packet_count: count,
                    average_packet_rate: Some(rate)
                }
                .validate()
                .is_err()
            );
        }
        assert!(
            serde_json::from_str::<ClassicPacketRateV1>(
                r#"{"schema_version":1,"packet_count":0,"average_packet_rate":null,"extra":true}"#
            )
            .is_err()
        );
    }

    #[test]
    fn retains_raw_fractional_rate_and_explicit_unknown() {
        for observation in [
            ClassicPacketRateV1 {
                schema_version: 1,
                packet_count: 100,
                average_packet_rate: Some(30000.0 / 1001.0),
            },
            ClassicPacketRateV1 {
                schema_version: 1,
                packet_count: 0,
                average_packet_rate: None,
            },
            ClassicPacketRateV1 {
                schema_version: 1,
                packet_count: 100,
                average_packet_rate: None,
            },
        ] {
            observation.validate().unwrap();
            let encoded = serde_json::to_string(&observation).unwrap();
            assert_eq!(
                serde_json::from_str::<ClassicPacketRateV1>(&encoded).unwrap(),
                observation
            );
        }
    }
}
