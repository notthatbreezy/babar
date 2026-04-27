//! Shared PostGIS value types.
//!
//! The v1 spatial surface uses `geo-types` for geometry data and keeps PostGIS
//! concerns explicit in small wrappers:
//!
//! - [`Geometry<T>`] for PostGIS `geometry`
//! - [`Geography<T>`] for PostGIS `geography`
//! - [`Srid`] for optional spatial-reference metadata
//!
//! This module intentionally does **not** model PostgreSQL's built-in geometric
//! types such as `point`, `line`, or `polygon`.

use geo_types::Geometry as GeoGeometry;

/// PostGIS spatial kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpatialKind {
    /// PostGIS `geometry`.
    Geometry,
    /// PostGIS `geography`.
    Geography,
}

/// Spatial reference identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Srid(u32);

impl Srid {
    /// WGS 84, the default SRID most commonly used with PostGIS `geography`.
    pub const WGS84: Self = Self(4326);

    /// Build an SRID wrapper.
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Access the raw SRID value.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// PostGIS `geometry` wrapper around a `geo-types` value plus optional SRID.
#[derive(Debug, Clone, PartialEq)]
pub struct Geometry<T = GeoGeometry<f64>> {
    value: T,
    srid: Option<Srid>,
}

impl<T> Geometry<T> {
    /// Wrap a geometry value without an explicit SRID.
    pub const fn new(value: T) -> Self {
        Self { value, srid: None }
    }

    /// Wrap a geometry value with an explicit SRID.
    pub const fn with_srid(value: T, srid: Srid) -> Self {
        Self {
            value,
            srid: Some(srid),
        }
    }

    /// PostGIS spatial kind carried by this wrapper.
    pub const fn kind(&self) -> SpatialKind {
        SpatialKind::Geometry
    }

    /// The optional SRID attached to this geometry.
    pub const fn srid(&self) -> Option<Srid> {
        self.srid
    }

    /// Borrow the wrapped `geo-types` value.
    pub const fn value(&self) -> &T {
        &self.value
    }

    /// Consume the wrapper and return the inner `geo-types` value.
    pub fn into_value(self) -> T {
        self.value
    }
}

impl<T> From<T> for Geometry<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

/// PostGIS `geography` wrapper around a `geo-types` value plus optional SRID.
#[derive(Debug, Clone, PartialEq)]
pub struct Geography<T = GeoGeometry<f64>> {
    value: T,
    srid: Option<Srid>,
}

impl<T> Geography<T> {
    /// Wrap a geography value without an explicit SRID.
    pub const fn new(value: T) -> Self {
        Self { value, srid: None }
    }

    /// Wrap a geography value with an explicit SRID.
    pub const fn with_srid(value: T, srid: Srid) -> Self {
        Self {
            value,
            srid: Some(srid),
        }
    }

    /// Build a geography value in the common WGS 84 SRID.
    pub const fn wgs84(value: T) -> Self {
        Self::with_srid(value, Srid::WGS84)
    }

    /// PostGIS spatial kind carried by this wrapper.
    pub const fn kind(&self) -> SpatialKind {
        SpatialKind::Geography
    }

    /// The optional SRID attached to this geography.
    pub const fn srid(&self) -> Option<Srid> {
        self.srid
    }

    /// Borrow the wrapped `geo-types` value.
    pub const fn value(&self) -> &T {
        &self.value
    }

    /// Consume the wrapper and return the inner `geo-types` value.
    pub fn into_value(self) -> T {
        self.value
    }
}

impl<T> From<T> for Geography<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

#[cfg(test)]
mod tests {
    use geo_types::{Geometry as GeoGeometry, Point};

    use super::{Geography, Geometry, SpatialKind, Srid};

    #[test]
    fn geometry_defaults_to_no_srid() {
        let value = Geometry::new(Point::new(1.0, 2.0));
        assert_eq!(value.kind(), SpatialKind::Geometry);
        assert_eq!(value.srid(), None);
    }

    #[test]
    fn geography_supports_common_wgs84_helper() {
        let value = Geography::wgs84(Point::new(1.0, 2.0));
        assert_eq!(value.kind(), SpatialKind::Geography);
        assert_eq!(value.srid(), Some(Srid::WGS84));
    }

    #[test]
    fn wrappers_default_to_geo_types_geometry_surface() {
        let value = Geometry::with_srid(GeoGeometry::from(Point::new(1.0, 2.0)), Srid::new(3857));
        assert_eq!(value.srid(), Some(Srid::new(3857)));
    }
}
