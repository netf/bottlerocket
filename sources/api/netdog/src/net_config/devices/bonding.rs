use super::validate_addressing;
use super::{error, Dhcp4ConfigV1, Dhcp6ConfigV1, Result, RouteV1, StaticConfigV1, Validate};
use crate::interface_name::InterfaceName;
use crate::net_config::devices::generate_addressing_validation;
use serde::de::Error;
use serde::{Deserialize, Deserializer};
use snafu::ensure;
use std::net::IpAddr;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(remote = "Self")]
pub(crate) struct NetBondV1 {
    pub(crate) primary: Option<bool>,
    pub(crate) dhcp4: Option<Dhcp4ConfigV1>,
    pub(crate) dhcp6: Option<Dhcp6ConfigV1>,
    pub(crate) static4: Option<StaticConfigV1>,
    pub(crate) static6: Option<StaticConfigV1>,
    #[serde(rename = "route")]
    pub(crate) routes: Option<Vec<RouteV1>>,
    kind: String,
    pub(crate) mode: BondMode,
    #[serde(rename = "min-links")]
    pub(crate) min_links: Option<usize>,
    #[serde(rename = "monitoring")]
    pub(crate) monitoring_config: BondMonitoringConfig,
    pub(crate) interfaces: Vec<InterfaceName>,
}

impl<'de> Deserialize<'de> for NetBondV1 {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let this = Self::deserialize(deserializer)?;
        if this.kind.to_lowercase().as_str() != "bond" {
            return Err(D::Error::custom(format!(
                "kind of '{}' does not match 'bond'",
                this.kind.as_str()
            )));
        }

        Ok(this)
    }
}

generate_addressing_validation!(&NetBondV1);

impl Validate for NetBondV1 {
    fn validate(&self) -> Result<()> {
        validate_addressing(self)?;

        // TODO: We should move this and other validation logic into Deserialize when messaging
        // is better for enum failures https://github.com/serde-rs/serde/issues/2157
        let interfaces_count = self.interfaces.len();
        ensure!(
            interfaces_count > 0,
            error::InvalidNetConfigSnafu {
                reason: "bonds must have 1 or more interfaces specified"
            }
        );
        if let Some(min_links) = self.min_links {
            ensure!(
                min_links <= interfaces_count,
                error::InvalidNetConfigSnafu {
                    reason: "min-links is greater than number of interfaces configured"
                }
            )
        }
        // Validate monitoring configuration
        match &self.monitoring_config {
            BondMonitoringConfig::MiiMon(config) => config.validate()?,
            BondMonitoringConfig::ArpMon(config) => config.validate()?,
        }

        Ok(())
    }
}

// Currently only mode 1 (active-backup) is supported but eventually 0-6 could be added
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum BondMode {
    ActiveBackup,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum BondMonitoringConfig {
    MiiMon(MiiMonitoringConfig),
    ArpMon(ArpMonitoringConfig),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MiiMonitoringConfig {
    #[serde(rename = "miimon-frequency-ms")]
    pub(crate) frequency: u32,
    #[serde(rename = "miimon-updelay-ms")]
    pub(crate) updelay: u32,
    #[serde(rename = "miimon-downdelay-ms")]
    pub(crate) downdelay: u32,
}

impl Validate for MiiMonitoringConfig {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.frequency > 0,
            error::InvalidNetConfigSnafu {
                reason: "miimon-frequency-ms of 0 disables Mii Monitoring, either set a value or configure Arp Monitoring"
            }
        );
        // updelay and downdelay should be a multiple of frequency, but will be rounded down
        // by the kernel, this ensures they are at least the size of frequency (non-zero)
        ensure!(
            self.frequency <= self.updelay && self.frequency <= self.downdelay,
            error::InvalidNetConfigSnafu {
                reason: "miimon-updelay-ms and miimon-downdelay-ms must be equal to or larger than miimon-frequency-ms"
            }
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ArpMonitoringConfig {
    #[serde(rename = "arpmon-interval-ms")]
    pub(crate) interval: u32,
    #[serde(rename = "arpmon-validate")]
    pub(crate) validate: ArpValidate,
    #[serde(rename = "arpmon-targets")]
    pub(crate) targets: Vec<IpAddr>,
}

impl Validate for ArpMonitoringConfig {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.interval > 0,
            error::InvalidNetConfigSnafu {
                reason: "arpmon-interval-ms of 0 disables Arp Monitoring, either set a value or configure Mii Monitoring"
            }
        );
        // If using Arp Monitoring, 1-16 targets must be specified
        let targets_length: u32 = self.targets.len() as u32;
        ensure!(
            targets_length > 0 && targets_length <= 16,
            error::InvalidNetConfigSnafu {
                reason: "arpmon-targets must include between 1 and 16 targets"
            }
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ArpValidate {
    Active,
    All,
    Backup,
    None,
}
