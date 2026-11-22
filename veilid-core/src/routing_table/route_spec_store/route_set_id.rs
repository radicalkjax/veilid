use super::*;

/// Identifies a specific allocated route set in the route spec store cache.
/// Wraps the underlying [RouteId] so allocated and remote IDs are not
/// interchangeable at compile time; the public veilid_api surface continues
/// to use [RouteId] and converts at the boundary. The on-disk serialized
/// shape is identical to [RouteId] via `serde(transparent)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
#[must_use]
pub(crate) struct AllocatedRouteSetId(RouteId);

impl AllocatedRouteSetId {
    pub fn from_route_id(id: RouteId) -> Self {
        Self(id)
    }
}

impl From<RouteId> for AllocatedRouteSetId {
    fn from(id: RouteId) -> Self {
        Self(id)
    }
}

impl From<AllocatedRouteSetId> for RouteId {
    fn from(id: AllocatedRouteSetId) -> Self {
        id.0
    }
}

impl fmt::Display for AllocatedRouteSetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", f.to_string(&self.0))
    }
}

/// Identifies a specific remote (imported) route set in the route spec store
/// cache. Wraps the underlying [RouteId] for compile-time separation from
/// [AllocatedRouteSetId]; veilid_api converts at the boundary. The on-disk
/// serialized shape is identical to [RouteId] via `serde(transparent)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
#[must_use]
pub(crate) struct RemoteRouteSetId(RouteId);

impl RemoteRouteSetId {
    pub fn from_route_id(id: RouteId) -> Self {
        Self(id)
    }
}

impl From<RouteId> for RemoteRouteSetId {
    fn from(id: RouteId) -> Self {
        Self(id)
    }
}

impl From<RemoteRouteSetId> for RouteId {
    fn from(id: RemoteRouteSetId) -> Self {
        id.0
    }
}

impl fmt::Display for RemoteRouteSetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", f.to_string(&self.0))
    }
}

/// A route set a route test targets, typed so a test requested for one kind of route set never
/// runs against the other: an import of our own route's blob shares the allocated route's id, and
/// a retest of that import must not test the allocated route once the import is gone
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[must_use]
pub(crate) enum RouteTestTarget {
    Allocated(AllocatedRouteSetId),
    Remote(RemoteRouteSetId),
}

impl From<AllocatedRouteSetId> for RouteTestTarget {
    fn from(id: AllocatedRouteSetId) -> Self {
        Self::Allocated(id)
    }
}

impl From<RemoteRouteSetId> for RouteTestTarget {
    fn from(id: RemoteRouteSetId) -> Self {
        Self::Remote(id)
    }
}

impl fmt::Display for RouteTestTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Allocated(id) => write!(f, "allocated {}", id),
            Self::Remote(id) => write!(f, "remote {}", id),
        }
    }
}
