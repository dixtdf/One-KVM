// SPDX-License-Identifier: AGPL-3.0-only
use one_kvm_platform_core as core;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub selector_kind: String,
    pub selector: String,
}

impl Resource {
    pub fn domain(&self) -> Result<core::ResourceBinding, &'static str> {
        if self.name.is_empty() || self.name.len() > 128 || self.selector.is_empty() {
            return Err("Invalid resource");
        }
        let kind = match self.kind.as_str() {
            "video" => core::ResourceKind::Video,
            "control" => core::ResourceKind::Control,
            _ => return Err("Unknown resource kind"),
        };
        let selector = match self.selector_kind.as_str() {
            "identity" => core::StableSelector::ByIdentity(self.selector.clone()),
            "port" => core::StableSelector::ByPhysicalPort(self.selector.clone()),
            "builtin" => core::StableSelector::Builtin(self.selector.clone()),
            _ => return Err("Unknown selector kind"),
        };
        Ok(core::ResourceBinding { id: core::ResourceId(self.id.clone()), kind, selector })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Channel {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub video: String,
    pub control: Option<String>,
    pub rustdesk_id: Option<String>,
}

impl Channel {
    pub fn domain(&self) -> core::Channel {
        core::Channel { id: core::ChannelId(self.id.clone()), enabled: self.enabled,
            video: core::ResourceId(self.video.clone()),
            control: self.control.clone().map(core::ResourceId), rustdesk_id: self.rustdesk_id.clone() }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct User {
    pub id: String,
    pub username: String,
    pub admin: bool,
    pub active: bool,
}

#[derive(Clone, Debug)]
pub struct Principal(pub User);

pub fn role(value: &str) -> Option<core::Role> {
    match value {
        "viewer" => Some(core::Role::Viewer),
        "operator" => Some(core::Role::Operator),
        "manager" => Some(core::Role::ChannelManager),
        _ => None,
    }
}
