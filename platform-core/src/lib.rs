// SPDX-License-Identifier: AGPL-3.0-only
//! Experimental domain model. No device I/O, credentials, database migration,
//! HTTP handler or production authentication is implemented by this crate.
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChannelId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ResourceId(pub String);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceKind {
    Video,
    Control,
}

/// A persistent OS alias, not a changing /dev/videoN, ttyUSBN or bus/device number.
/// The device discovery adapter must produce the same key for the same identity.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum StableSelector {
    ByIdentity(String),
    ByPhysicalPort(String),
    Builtin(String),
}

impl StableSelector {
    fn value(&self) -> &str {
        match self {
            Self::ByIdentity(s) | Self::ByPhysicalPort(s) | Self::Builtin(s) => s,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ResourceBinding {
    pub id: ResourceId,
    pub kind: ResourceKind,
    pub selector: StableSelector,
}

/// Constructed by trusted discovery, never from an untrusted user's path.
#[derive(Clone, Debug)]
pub struct Observation {
    /// Stable identity of this particular interface, shared across its aliases.
    pub physical_interface: String,
    pub selectors: Vec<StableSelector>,
    pub kind: ResourceKind,
    pub current_path: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ResolveError {
    Missing,
    Ambiguous,
    InvalidObservation,
}

pub fn resolve<'a>(
    binding: &ResourceBinding,
    inventory: &'a [Observation],
) -> Result<&'a Observation, ResolveError> {
    let mut matches = inventory.iter().filter(|d| {
        d.kind == binding.kind && d.selectors.contains(&binding.selector)
    });
    let first = matches.next().ok_or(ResolveError::Missing)?;
    if matches.next().is_some() {
        return Err(ResolveError::Ambiguous);
    }
    if first.physical_interface.is_empty() || first.current_path.is_empty() {
        return Err(ResolveError::InvalidObservation);
    }
    Ok(first)
}

#[derive(Clone, Debug)]
pub struct Channel {
    /// Internal persistent ID, independent of the user-entered RustDesk ID.
    pub id: ChannelId,
    pub enabled: bool,
    pub video: ResourceId,
    pub control: Option<ResourceId>,
    /// Sent verbatim. No numeric-only or other format restriction is added.
    pub rustdesk_id: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ConfigError {
    InvalidId,
    DuplicateChannel,
    DuplicateResource,
    MissingResource,
    WrongResourceKind,
    ConflictingBinding,
    DuplicateRustDeskId,
}

pub fn validate_config(channels: &[Channel], bindings: &[ResourceBinding]) -> Result<(), ConfigError> {
    let mut resources = BTreeMap::new();
    for binding in bindings {
        if binding.id.0.is_empty() || binding.selector.value().is_empty() {
            return Err(ConfigError::InvalidId);
        }
        if resources.insert(binding.id.clone(), binding).is_some() {
            return Err(ConfigError::DuplicateResource);
        }
    }
    let mut ids = BTreeSet::new();
    let mut claims = BTreeSet::new();
    let mut endpoints = BTreeSet::new();
    for channel in channels {
        if channel.id.0.is_empty() {
            return Err(ConfigError::InvalidId);
        }
        if !ids.insert(channel.id.clone()) {
            return Err(ConfigError::DuplicateChannel);
        }
        for (id, expected) in std::iter::once((&channel.video, ResourceKind::Video))
            .chain(channel.control.iter().map(|id| (id, ResourceKind::Control)))
        {
            let binding = resources.get(id).ok_or(ConfigError::MissingResource)?;
            if binding.kind != expected {
                return Err(ConfigError::WrongResourceKind);
            }
            if channel.enabled && !claims.insert(binding.selector.clone()) {
                return Err(ConfigError::ConflictingBinding);
            }
        }
        if channel.enabled {
            if let Some(id) = &channel.rustdesk_id {
                if !endpoints.insert(id) {
                    return Err(ConfigError::DuplicateRustDeskId);
                }
            }
        }
    }
    Ok(())
}

/// Resolve all required resources atomically before starting a worker. Aliases
/// for the same physical interface cannot be used to bypass exclusive ownership.
pub fn resolve_enabled<'a>(
    channels: &[Channel],
    bindings: &[ResourceBinding],
    inventory: &'a [Observation],
) -> Result<BTreeMap<ResourceId, &'a Observation>, StartError> {
    validate_config(channels, bindings).map_err(StartError::Config)?;
    let resources: BTreeMap<_, _> = bindings.iter().map(|r| (&r.id, r)).collect();
    let mut physical = BTreeSet::new();
    let mut resolved = BTreeMap::new();
    for channel in channels.iter().filter(|c| c.enabled) {
        for id in std::iter::once(&channel.video).chain(channel.control.iter()) {
            let device = resolve(resources[id], inventory)
                .map_err(|error| StartError::Device(id.clone(), error))?;
            if !physical.insert(device.physical_interface.clone()) {
                return Err(StartError::PhysicalConflict);
            }
            resolved.insert(id.clone(), device);
        }
    }
    Ok(resolved)
}

#[derive(Debug, PartialEq, Eq)]
pub enum StartError {
    Config(ConfigError),
    Device(ResourceId, ResolveError),
    PhysicalConflict,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    View,
    Control,
    Configure,
}

#[derive(Clone, Copy, Debug)]
pub enum Role {
    Viewer,
    Operator,
    ChannelManager,
}

impl Role {
    fn allows(self, action: Permission) -> bool {
        matches!((self, action),
            (Self::Viewer, Permission::View)
            | (Self::Operator, Permission::View | Permission::Control)
            | (Self::ChannelManager, _))
    }
}

#[derive(Default)]
pub struct AccessPolicy {
    // Only the administrative service may mutate grants. A channel manager
    // configures its channel; it does not manage users or grant itself access.
    grants: BTreeMap<(String, ChannelId), Role>,
}

impl AccessPolicy {
    pub fn grant(&mut self, user: String, channel: ChannelId, role: Role) {
        self.grants.insert((user, channel), role);
    }

    pub fn revoke(&mut self, user: &str, channel: &ChannelId) {
        self.grants.remove(&(user.to_owned(), channel.clone()));
    }

    /// Caller must supply an authenticated principal and current policy on each
    /// request/input event. This does not authenticate a username or RustDesk peer.
    pub fn allows(&self, user: &str, channel: &Channel, action: Permission) -> bool {
        channel.enabled && self.grants.get(&(user.to_owned(), channel.id.clone()))
            .is_some_and(|role| role.allows(action))
            && (action != Permission::Control || channel.control.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(n: usize) -> (Channel, ResourceBinding, Observation) {
        let binding = ResourceBinding { id: ResourceId(format!("video-{n}")),
            kind: ResourceKind::Video, selector: StableSelector::ByPhysicalPort(format!("port-{n}")) };
        let observation = Observation { physical_interface: format!("physical-{n}"),
            selectors: vec![binding.selector.clone()], kind: ResourceKind::Video,
            current_path: format!("/dev/video{n}") };
        let channel = Channel { id: ChannelId(format!("channel-{n}")), enabled: true,
            video: binding.id.clone(), control: None, rustdesk_id: Some(format!(" 自定义-{n} ")) };
        (channel, binding, observation)
    }

    #[test]
    fn reenumeration_keeps_binding_and_missing_never_falls_back() {
        let (_, binding, mut observed) = fixture(0);
        observed.current_path = "/dev/video78".into();
        let inventory = vec![observed];
        assert_eq!(resolve(&binding, &inventory).unwrap().current_path, "/dev/video78");
        let (_, _, other) = fixture(1);
        assert_eq!(resolve(&binding, &[other]).unwrap_err(), ResolveError::Missing);
    }

    #[test]
    fn duplicate_identity_and_wrong_kind_fail_closed() {
        let (_, binding, device) = fixture(0);
        assert_eq!(resolve(&binding, &[device.clone(), device.clone()]).unwrap_err(), ResolveError::Ambiguous);
        let mut wrong = device;
        wrong.kind = ResourceKind::Control;
        assert_eq!(resolve(&binding, &[wrong]).unwrap_err(), ResolveError::Missing);
    }

    #[test]
    fn channel_count_has_no_product_limit() {
        let fixtures: Vec<_> = (0..128).map(fixture).collect();
        let channels: Vec<_> = fixtures.iter().map(|f| f.0.clone()).collect();
        let bindings: Vec<_> = fixtures.iter().map(|f| f.1.clone()).collect();
        let inventory: Vec<_> = fixtures.iter().map(|f| f.2.clone()).collect();
        assert_eq!(resolve_enabled(&channels, &bindings, &inventory).unwrap().len(), 128);
    }

    #[test]
    fn two_channels_cannot_claim_same_binding_or_endpoint() {
        let (a, binding, _) = fixture(0);
        let mut b = a.clone(); b.id = ChannelId("other".into());
        assert_eq!(validate_config(&[a.clone(), b], &[binding.clone()]), Err(ConfigError::ConflictingBinding));
        let (mut b, second, _) = fixture(1); b.rustdesk_id = a.rustdesk_id.clone();
        assert_eq!(validate_config(&[a, b], &[binding, second]), Err(ConfigError::DuplicateRustDeskId));
    }

    #[test]
    fn aliases_cannot_double_claim_the_same_physical_interface() {
        let (a, binding, mut device) = fixture(0);
        let (b, second, _) = fixture(1); device.selectors.push(second.selector.clone());
        assert_eq!(resolve_enabled(&[a, b], &[binding, second], &[device]).unwrap_err(), StartError::PhysicalConflict);
    }

    #[test]
    fn viewer_cannot_control_and_grants_are_channel_scoped() {
        let (mut a, _, _) = fixture(0); a.control = Some(ResourceId("otg".into()));
        let (b, _, _) = fixture(1); let mut policy = AccessPolicy::default();
        policy.grant("alice".into(), a.id.clone(), Role::Viewer);
        assert!(policy.allows("alice", &a, Permission::View));
        assert!(!policy.allows("alice", &a, Permission::Control));
        assert!(!policy.allows("alice", &b, Permission::View));
        assert!(!policy.allows("unknown", &a, Permission::View));
        policy.grant("alice".into(), a.id.clone(), Role::Operator);
        assert!(policy.allows("alice", &a, Permission::Control));
        assert!(!policy.allows("alice", &a, Permission::Configure));
        policy.revoke("alice", &a.id);
        assert!(!policy.allows("alice", &a, Permission::Control));
    }

    #[test]
    fn disabled_or_video_only_channels_do_not_accept_input() {
        let (mut channel, _, _) = fixture(0); let mut policy = AccessPolicy::default();
        policy.grant("manager".into(), channel.id.clone(), Role::ChannelManager);
        assert!(!policy.allows("manager", &channel, Permission::Control));
        channel.enabled = false;
        assert!(!policy.allows("manager", &channel, Permission::View));
    }

    #[test]
    fn broken_configuration_is_rejected_before_start() {
        let (channel, mut binding, _) = fixture(0);
        assert_eq!(validate_config(&[channel.clone()], &[]), Err(ConfigError::MissingResource));
        binding.kind = ResourceKind::Control;
        assert_eq!(validate_config(&[channel], &[binding]), Err(ConfigError::WrongResourceKind));
    }
}
