//! PostGIS `geometry` / `geography` codecs and shared value types.
//!
//! The v1 spatial surface uses `geo-types` for geometry data and keeps PostGIS
//! concerns explicit in small wrappers:
//!
//! - [`Geometry<T>`] for PostGIS `geometry`
//! - [`Geography<T>`] for PostGIS `geography`
//! - [`Srid`] for optional spatial-reference metadata
//!
//! The codecs speak PostGIS EWKB over PostgreSQL's binary protocol and support
//! common 2D `geo-types` shapes:
//!
//! - `Point`
//! - `LineString`
//! - `Polygon`
//! - `MultiPoint`
//! - `MultiLineString`
//! - `MultiPolygon`
//! - `geo_types::Geometry<f64>` when it contains one of the above variants
//!
//! Deliberate v1 limitations:
//!
//! - 2D only — Z / M / ZM EWKB is rejected
//! - no `GeometryCollection`, `Line`, `Rect`, or `Triangle`
//! - no PostgreSQL built-in geometric types such as `point`, `line`, or
//!   `polygon`

use std::convert::TryFrom;
use std::marker::PhantomData;

use bytes::{Bytes, BytesMut};
use geo_types::{
    Geometry as GeoGeometry, LineString, MultiLineString, MultiPoint, MultiPolygon, Point, Polygon,
};

use super::{Decoder, Encoder, FORMAT_BINARY};
use crate::error::{Error, Result};
use crate::types::{self, Oid, Type};

const DYNAMIC_OIDS: [Oid; 1] = [0];
const GEOMETRY_TYPES: [Type; 1] = [types::GEOMETRY_TYPE];
const GEOGRAPHY_TYPES: [Type; 1] = [types::GEOGRAPHY_TYPE];

const WKB_POINT: u32 = 1;
const WKB_LINESTRING: u32 = 2;
const WKB_POLYGON: u32 = 3;
const WKB_MULTIPOINT: u32 = 4;
const WKB_MULTILINESTRING: u32 = 5;
const WKB_MULTIPOLYGON: u32 = 6;
const WKB_GEOMETRY_COLLECTION: u32 = 7;

const EWKB_Z_FLAG: u32 = 0x8000_0000;
const EWKB_M_FLAG: u32 = 0x4000_0000;
const EWKB_SRID_FLAG: u32 = 0x2000_0000;
const EWKB_TYPE_MASK: u32 = 0x0000_FFFF;

/// PostGIS spatial kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpatialKind {
    /// PostGIS `geometry`.
    Geometry,
    /// PostGIS `geography`.
    Geography,
}

impl SpatialKind {
    fn sql_name(self) -> &'static str {
        match self {
            Self::Geometry => "geometry",
            Self::Geography => "geography",
        }
    }
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

/// Codec for PostGIS `geometry`.
#[derive(Debug, Clone, Copy)]
pub struct GeometryCodec<T>(PhantomData<fn() -> T>);

/// Codec for PostGIS `geography`.
#[derive(Debug, Clone, Copy)]
pub struct GeographyCodec<T>(PhantomData<fn() -> T>);

/// Build a PostGIS `geometry` codec for a supported `geo-types` shape.
pub fn geometry<T>() -> GeometryCodec<T> {
    GeometryCodec(PhantomData)
}

/// Build a PostGIS `geography` codec for a supported `geo-types` shape.
pub fn geography<T>() -> GeographyCodec<T> {
    GeographyCodec(PhantomData)
}

trait GeoValue: Sized {
    fn into_geometry(self) -> GeoGeometry<f64>;
    fn try_from_geometry(geometry: GeoGeometry<f64>) -> Result<Self>;
}

impl GeoValue for GeoGeometry<f64> {
    fn into_geometry(self) -> GeoGeometry<f64> {
        self
    }

    fn try_from_geometry(geometry: GeoGeometry<f64>) -> Result<Self> {
        Ok(geometry)
    }
}

macro_rules! impl_geo_value {
    ($ty:ty, $variant:ident, $name:literal) => {
        impl GeoValue for $ty {
            fn into_geometry(self) -> GeoGeometry<f64> {
                self.into()
            }

            fn try_from_geometry(geometry: GeoGeometry<f64>) -> Result<Self> {
                match geometry {
                    GeoGeometry::$variant(value) => Ok(value),
                    other => Err(Error::Codec(format!(
                        "postgis: expected {}, got {}",
                        $name,
                        geometry_variant_name(&other)
                    ))),
                }
            }
        }
    };
}

impl_geo_value!(Point<f64>, Point, "Point");
impl_geo_value!(LineString<f64>, LineString, "LineString");
impl_geo_value!(Polygon<f64>, Polygon, "Polygon");
impl_geo_value!(MultiPoint<f64>, MultiPoint, "MultiPoint");
impl_geo_value!(MultiLineString<f64>, MultiLineString, "MultiLineString");
impl_geo_value!(MultiPolygon<f64>, MultiPolygon, "MultiPolygon");

impl<T> Encoder<Geometry<T>> for GeometryCodec<T>
where
    T: Clone + GeoValue,
{
    fn encode(&self, value: &Geometry<T>, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        let geometry = value.value.clone().into_geometry();
        params.push(Some(encode_spatial(
            SpatialKind::Geometry,
            value.srid,
            &geometry,
        )?));
        Ok(())
    }

    fn oids(&self) -> &'static [Oid] {
        &DYNAMIC_OIDS
    }

    fn types(&self) -> &'static [Type] {
        &GEOMETRY_TYPES
    }

    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

impl<T> Decoder<Geometry<T>> for GeometryCodec<T>
where
    T: GeoValue,
{
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<Geometry<T>> {
        let value = decode_spatial(columns, SpatialKind::Geometry)?;
        Ok(Geometry {
            value: T::try_from_geometry(value.geometry)?,
            srid: value.srid,
        })
    }

    fn n_columns(&self) -> usize {
        1
    }

    fn oids(&self) -> &'static [Oid] {
        &DYNAMIC_OIDS
    }

    fn types(&self) -> &'static [Type] {
        &GEOMETRY_TYPES
    }

    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

impl<T> Encoder<Geography<T>> for GeographyCodec<T>
where
    T: Clone + GeoValue,
{
    fn encode(&self, value: &Geography<T>, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        let geometry = value.value.clone().into_geometry();
        params.push(Some(encode_spatial(
            SpatialKind::Geography,
            value.srid,
            &geometry,
        )?));
        Ok(())
    }

    fn oids(&self) -> &'static [Oid] {
        &DYNAMIC_OIDS
    }

    fn types(&self) -> &'static [Type] {
        &GEOGRAPHY_TYPES
    }

    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

impl<T> Decoder<Geography<T>> for GeographyCodec<T>
where
    T: GeoValue,
{
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<Geography<T>> {
        let value = decode_spatial(columns, SpatialKind::Geography)?;
        Ok(Geography {
            value: T::try_from_geometry(value.geometry)?,
            srid: value.srid,
        })
    }

    fn n_columns(&self) -> usize {
        1
    }

    fn oids(&self) -> &'static [Oid] {
        &DYNAMIC_OIDS
    }

    fn types(&self) -> &'static [Type] {
        &GEOGRAPHY_TYPES
    }

    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

struct DecodedSpatial {
    geometry: GeoGeometry<f64>,
    srid: Option<Srid>,
}

fn encode_spatial(
    kind: SpatialKind,
    srid: Option<Srid>,
    geometry: &GeoGeometry<f64>,
) -> Result<Vec<u8>> {
    let mut buf = BytesMut::new();
    write_geometry(&mut buf, geometry, srid, true, kind)?;
    Ok(buf.to_vec())
}

fn decode_spatial(columns: &[Option<Bytes>], kind: SpatialKind) -> Result<DecodedSpatial> {
    let bytes = columns
        .first()
        .ok_or_else(|| {
            Error::Codec(format!(
                "postgis {}: decoder needs 1 column, got 0",
                kind.sql_name()
            ))
        })?
        .as_deref()
        .ok_or_else(|| {
            Error::Codec(format!(
                "postgis {}: unexpected NULL; use nullable() to allow it",
                kind.sql_name()
            ))
        })?;

    let mut input = bytes;
    let decoded = read_geometry(&mut input, kind)?;
    if !input.is_empty() {
        return Err(Error::Codec(format!(
            "postgis {}: unexpected trailing EWKB bytes",
            kind.sql_name()
        )));
    }
    Ok(decoded)
}

fn write_geometry(
    out: &mut BytesMut,
    geometry: &GeoGeometry<f64>,
    srid: Option<Srid>,
    include_srid: bool,
    kind: SpatialKind,
) -> Result<()> {
    out.extend_from_slice(&[1]);

    let mut type_id = match geometry {
        GeoGeometry::Point(_) => WKB_POINT,
        GeoGeometry::LineString(_) => WKB_LINESTRING,
        GeoGeometry::Polygon(_) => WKB_POLYGON,
        GeoGeometry::MultiPoint(_) => WKB_MULTIPOINT,
        GeoGeometry::MultiLineString(_) => WKB_MULTILINESTRING,
        GeoGeometry::MultiPolygon(_) => WKB_MULTIPOLYGON,
        GeoGeometry::GeometryCollection(_) => {
            return Err(v1_limitation(kind, "GeometryCollection"));
        }
        GeoGeometry::Line(_) => return Err(v1_limitation(kind, "Line")),
        GeoGeometry::Rect(_) => return Err(v1_limitation(kind, "Rect")),
        GeoGeometry::Triangle(_) => return Err(v1_limitation(kind, "Triangle")),
    };

    if include_srid && srid.is_some() {
        type_id |= EWKB_SRID_FLAG;
    }

    out.extend_from_slice(&type_id.to_le_bytes());
    if include_srid {
        if let Some(srid) = srid {
            out.extend_from_slice(&srid.get().to_le_bytes());
        }
    }

    match geometry {
        GeoGeometry::Point(point) => write_point(out, point),
        GeoGeometry::LineString(line) => write_line_string(out, line),
        GeoGeometry::Polygon(polygon) => write_polygon(out, polygon),
        GeoGeometry::MultiPoint(points) => {
            out.extend_from_slice(
                &usize_to_u32(points.0.len(), kind, "MultiPoint count")?.to_le_bytes(),
            );
            for point in &points.0 {
                write_geometry(out, &GeoGeometry::Point(*point), None, false, kind)?;
            }
        }
        GeoGeometry::MultiLineString(lines) => {
            out.extend_from_slice(
                &usize_to_u32(lines.0.len(), kind, "MultiLineString count")?.to_le_bytes(),
            );
            for line in &lines.0 {
                write_geometry(
                    out,
                    &GeoGeometry::LineString(line.clone()),
                    None,
                    false,
                    kind,
                )?;
            }
        }
        GeoGeometry::MultiPolygon(polygons) => {
            out.extend_from_slice(
                &usize_to_u32(polygons.0.len(), kind, "MultiPolygon count")?.to_le_bytes(),
            );
            for polygon in &polygons.0 {
                write_geometry(
                    out,
                    &GeoGeometry::Polygon(polygon.clone()),
                    None,
                    false,
                    kind,
                )?;
            }
        }
        GeoGeometry::GeometryCollection(_)
        | GeoGeometry::Line(_)
        | GeoGeometry::Rect(_)
        | GeoGeometry::Triangle(_) => unreachable!(),
    }

    Ok(())
}

fn write_point(out: &mut BytesMut, point: &Point<f64>) {
    out.extend_from_slice(&point.x().to_le_bytes());
    out.extend_from_slice(&point.y().to_le_bytes());
}

fn write_line_string(out: &mut BytesMut, line: &LineString<f64>) {
    out.extend_from_slice(
        &u32::try_from(line.0.len())
            .expect("LineString coordinate count exceeds EWKB u32 limit")
            .to_le_bytes(),
    );
    for coord in &line.0 {
        out.extend_from_slice(&coord.x.to_le_bytes());
        out.extend_from_slice(&coord.y.to_le_bytes());
    }
}

fn write_polygon(out: &mut BytesMut, polygon: &Polygon<f64>) {
    let ring_count = 1 + polygon.interiors().len();
    out.extend_from_slice(
        &u32::try_from(ring_count)
            .expect("Polygon ring count exceeds EWKB u32 limit")
            .to_le_bytes(),
    );
    write_line_string(out, polygon.exterior());
    for ring in polygon.interiors() {
        write_line_string(out, ring);
    }
}

fn read_geometry(input: &mut &[u8], kind: SpatialKind) -> Result<DecodedSpatial> {
    let endian = read_u8(input)?;
    let little_endian = match endian {
        0 => false,
        1 => true,
        other => {
            return Err(Error::Codec(format!(
                "postgis {}: invalid EWKB endian marker {other}",
                kind.sql_name()
            )));
        }
    };

    let type_id = read_u32(input, little_endian)?;
    if type_id & EWKB_Z_FLAG != 0 || type_id & EWKB_M_FLAG != 0 {
        return Err(Error::Codec(format!(
            "postgis {}: only 2D EWKB is supported in v1",
            kind.sql_name()
        )));
    }

    let srid = if type_id & EWKB_SRID_FLAG != 0 {
        let srid = read_i32(input, little_endian)?;
        if srid < 0 {
            return Err(Error::Codec(format!(
                "postgis {}: negative SRIDs are not supported (got {srid})",
                kind.sql_name()
            )));
        }
        Some(Srid::new(
            u32::try_from(srid).expect("non-negative SRID should fit into u32"),
        ))
    } else {
        None
    };

    let geometry = match type_id & EWKB_TYPE_MASK {
        WKB_POINT => GeoGeometry::Point(read_point(input, little_endian)?),
        WKB_LINESTRING => GeoGeometry::LineString(read_line_string(input, little_endian)?),
        WKB_POLYGON => GeoGeometry::Polygon(read_polygon(input, little_endian)?),
        WKB_MULTIPOINT => GeoGeometry::MultiPoint(read_multi_point(input, little_endian, kind)?),
        WKB_MULTILINESTRING => {
            GeoGeometry::MultiLineString(read_multi_line_string(input, little_endian, kind)?)
        }
        WKB_MULTIPOLYGON => {
            GeoGeometry::MultiPolygon(read_multi_polygon(input, little_endian, kind)?)
        }
        WKB_GEOMETRY_COLLECTION => return Err(v1_limitation(kind, "GeometryCollection")),
        other => {
            return Err(Error::Codec(format!(
                "postgis {}: unsupported EWKB type id {other}",
                kind.sql_name()
            )));
        }
    };

    Ok(DecodedSpatial { geometry, srid })
}

fn read_point(input: &mut &[u8], little_endian: bool) -> Result<Point<f64>> {
    Ok(Point::new(
        read_f64(input, little_endian)?,
        read_f64(input, little_endian)?,
    ))
}

fn read_line_string(input: &mut &[u8], little_endian: bool) -> Result<LineString<f64>> {
    let count = read_u32(input, little_endian)? as usize;
    let mut coords = Vec::with_capacity(count);
    for _ in 0..count {
        coords.push((
            read_f64(input, little_endian)?,
            read_f64(input, little_endian)?,
        ));
    }
    Ok(LineString::from(coords))
}

fn read_polygon(input: &mut &[u8], little_endian: bool) -> Result<Polygon<f64>> {
    let ring_count = read_u32(input, little_endian)? as usize;
    if ring_count == 0 {
        return Ok(Polygon::new(LineString::new(vec![]), vec![]));
    }

    let exterior = read_line_string(input, little_endian)?;
    let mut interiors = Vec::with_capacity(ring_count.saturating_sub(1));
    for _ in 1..ring_count {
        interiors.push(read_line_string(input, little_endian)?);
    }
    Ok(Polygon::new(exterior, interiors))
}

fn read_multi_point(
    input: &mut &[u8],
    little_endian: bool,
    kind: SpatialKind,
) -> Result<MultiPoint<f64>> {
    let count = read_u32(input, little_endian)? as usize;
    let mut points = Vec::with_capacity(count);
    for _ in 0..count {
        let nested = read_geometry(input, kind)?;
        if nested.srid.is_some() {
            return Err(Error::Codec(format!(
                "postgis {}: nested SRIDs are not supported in v1",
                kind.sql_name()
            )));
        }
        points.push(match nested.geometry {
            GeoGeometry::Point(point) => point,
            other => {
                return Err(Error::Codec(format!(
                    "postgis {}: expected Point inside MultiPoint, got {}",
                    kind.sql_name(),
                    geometry_variant_name(&other)
                )));
            }
        });
    }
    Ok(points.into())
}

fn read_multi_line_string(
    input: &mut &[u8],
    little_endian: bool,
    kind: SpatialKind,
) -> Result<MultiLineString<f64>> {
    let count = read_u32(input, little_endian)? as usize;
    let mut lines = Vec::with_capacity(count);
    for _ in 0..count {
        let nested = read_geometry(input, kind)?;
        if nested.srid.is_some() {
            return Err(Error::Codec(format!(
                "postgis {}: nested SRIDs are not supported in v1",
                kind.sql_name()
            )));
        }
        lines.push(match nested.geometry {
            GeoGeometry::LineString(line) => line,
            other => {
                return Err(Error::Codec(format!(
                    "postgis {}: expected LineString inside MultiLineString, got {}",
                    kind.sql_name(),
                    geometry_variant_name(&other)
                )));
            }
        });
    }
    Ok(MultiLineString(lines))
}

fn read_multi_polygon(
    input: &mut &[u8],
    little_endian: bool,
    kind: SpatialKind,
) -> Result<MultiPolygon<f64>> {
    let count = read_u32(input, little_endian)? as usize;
    let mut polygons = Vec::with_capacity(count);
    for _ in 0..count {
        let nested = read_geometry(input, kind)?;
        if nested.srid.is_some() {
            return Err(Error::Codec(format!(
                "postgis {}: nested SRIDs are not supported in v1",
                kind.sql_name()
            )));
        }
        polygons.push(match nested.geometry {
            GeoGeometry::Polygon(polygon) => polygon,
            other => {
                return Err(Error::Codec(format!(
                    "postgis {}: expected Polygon inside MultiPolygon, got {}",
                    kind.sql_name(),
                    geometry_variant_name(&other)
                )));
            }
        });
    }
    Ok(MultiPolygon(polygons))
}

fn read_u8(input: &mut &[u8]) -> Result<u8> {
    Ok(read_exact(input, 1)?[0])
}

fn read_u32(input: &mut &[u8], little_endian: bool) -> Result<u32> {
    let raw = read_exact(input, 4)?;
    Ok(if little_endian {
        u32::from_le_bytes(raw.try_into().expect("4-byte slice"))
    } else {
        u32::from_be_bytes(raw.try_into().expect("4-byte slice"))
    })
}

fn read_i32(input: &mut &[u8], little_endian: bool) -> Result<i32> {
    let raw = read_exact(input, 4)?;
    Ok(if little_endian {
        i32::from_le_bytes(raw.try_into().expect("4-byte slice"))
    } else {
        i32::from_be_bytes(raw.try_into().expect("4-byte slice"))
    })
}

fn read_f64(input: &mut &[u8], little_endian: bool) -> Result<f64> {
    let raw = read_exact(input, 8)?;
    Ok(if little_endian {
        f64::from_le_bytes(raw.try_into().expect("8-byte slice"))
    } else {
        f64::from_be_bytes(raw.try_into().expect("8-byte slice"))
    })
}

fn read_exact<'a>(input: &mut &'a [u8], len: usize) -> Result<&'a [u8]> {
    if input.len() < len {
        return Err(Error::Codec(
            "postgis: truncated EWKB payload while decoding spatial value".into(),
        ));
    }
    let (head, tail) = input.split_at(len);
    *input = tail;
    Ok(head)
}

fn geometry_variant_name(geometry: &GeoGeometry<f64>) -> &'static str {
    match geometry {
        GeoGeometry::Point(_) => "Point",
        GeoGeometry::Line(_) => "Line",
        GeoGeometry::LineString(_) => "LineString",
        GeoGeometry::Polygon(_) => "Polygon",
        GeoGeometry::MultiPoint(_) => "MultiPoint",
        GeoGeometry::MultiLineString(_) => "MultiLineString",
        GeoGeometry::MultiPolygon(_) => "MultiPolygon",
        GeoGeometry::GeometryCollection(_) => "GeometryCollection",
        GeoGeometry::Rect(_) => "Rect",
        GeoGeometry::Triangle(_) => "Triangle",
    }
}

fn v1_limitation(kind: SpatialKind, geometry_name: &str) -> Error {
    Error::Codec(format!(
        "postgis {}: {} is intentionally unsupported in v1",
        kind.sql_name(),
        geometry_name
    ))
}

fn usize_to_u32(value: usize, kind: SpatialKind, field: &str) -> Result<u32> {
    u32::try_from(value).map_err(|_| {
        Error::Codec(format!(
            "postgis {}: {field} exceeds EWKB u32 limit",
            kind.sql_name()
        ))
    })
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use geo_types::{
        point, Geometry as GeoGeometry, LineString, MultiLineString, MultiPoint, MultiPolygon,
        Point, Polygon,
    };

    use super::{geography, geometry, Decoder, Encoder, Geography, Geometry, Srid, FORMAT_BINARY};
    use crate::types;

    #[test]
    fn geometry_defaults_to_no_srid() {
        let value = Geometry::new(Point::new(1.0, 2.0));
        assert_eq!(value.srid(), None);
    }

    #[test]
    fn geography_supports_common_wgs84_helper() {
        let value = Geography::wgs84(Point::new(1.0, 2.0));
        assert_eq!(value.srid(), Some(Srid::WGS84));
    }

    #[test]
    fn wrappers_default_to_geo_types_geometry_surface() {
        let value = Geometry::with_srid(GeoGeometry::from(Point::new(1.0, 2.0)), Srid::new(3857));
        assert_eq!(value.srid(), Some(Srid::new(3857)));
    }

    #[test]
    fn geometry_codec_uses_dynamic_postgis_type_metadata() {
        let codec = geometry::<Point<f64>>();
        assert_eq!(Encoder::<Geometry<Point<f64>>>::oids(&codec), &[0]);
        assert_eq!(
            Encoder::<Geometry<Point<f64>>>::types(&codec),
            &[types::GEOMETRY_TYPE]
        );
        assert_eq!(
            Decoder::<Geometry<Point<f64>>>::types(&codec),
            &[types::GEOMETRY_TYPE]
        );
        assert_eq!(
            Encoder::<Geometry<Point<f64>>>::format_codes(&codec),
            &[FORMAT_BINARY]
        );
    }

    #[test]
    fn geography_codec_uses_dynamic_postgis_type_metadata() {
        let codec = geography::<Point<f64>>();
        assert_eq!(Encoder::<Geography<Point<f64>>>::oids(&codec), &[0]);
        assert_eq!(
            Encoder::<Geography<Point<f64>>>::types(&codec),
            &[types::GEOGRAPHY_TYPE]
        );
        assert_eq!(
            Decoder::<Geography<Point<f64>>>::types(&codec),
            &[types::GEOGRAPHY_TYPE]
        );
        assert_eq!(
            Encoder::<Geography<Point<f64>>>::format_codes(&codec),
            &[FORMAT_BINARY]
        );
    }

    #[test]
    fn point_roundtrips_through_ewkb_codec() {
        let codec = geometry::<Point<f64>>();
        let value = Geometry::with_srid(point!(x: 1.25, y: -3.5), Srid::new(3857));
        let decoded = roundtrip(&codec, &value);
        assert_eq!(decoded, value);
    }

    #[test]
    fn common_shapes_roundtrip_through_generic_geometry_codec() {
        let codec = geometry::<GeoGeometry<f64>>();
        let values = [
            Geometry::new(GeoGeometry::from(LineString::from(vec![
                (0.0, 0.0),
                (2.0, 2.0),
            ]))),
            Geometry::with_srid(
                GeoGeometry::from(Polygon::new(
                    LineString::from(vec![(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 0.0)]),
                    vec![LineString::from(vec![
                        (1.0, 1.0),
                        (2.0, 1.0),
                        (1.0, 2.0),
                        (1.0, 1.0),
                    ])],
                )),
                Srid::new(4326),
            ),
            Geometry::new(GeoGeometry::from(MultiPoint::from(vec![
                (0.0, 0.0),
                (1.0, 1.0),
            ]))),
            Geometry::new(GeoGeometry::from(MultiLineString(vec![
                LineString::from(vec![(0.0, 0.0), (1.0, 0.0)]),
                LineString::from(vec![(2.0, 2.0), (3.0, 3.0)]),
            ]))),
            Geometry::new(GeoGeometry::from(MultiPolygon(vec![Polygon::new(
                LineString::from(vec![(10.0, 10.0), (12.0, 10.0), (12.0, 12.0), (10.0, 10.0)]),
                vec![],
            )]))),
        ];

        for value in values {
            let decoded = roundtrip(&codec, &value);
            assert_eq!(decoded, value);
        }
    }

    #[test]
    fn geography_roundtrips_point_ewkb() {
        let codec = geography::<Point<f64>>();
        let value = Geography::wgs84(Point::new(-73.9857, 40.7484));
        let decoded = roundtrip(&codec, &value);
        assert_eq!(decoded, value);
    }

    #[test]
    fn z_dimension_is_rejected() {
        let codec = geometry::<GeoGeometry<f64>>();
        let mut bytes = Vec::new();
        bytes.push(1);
        bytes.extend_from_slice(&(super::WKB_POINT | super::EWKB_Z_FLAG).to_le_bytes());
        bytes.extend_from_slice(&1.0f64.to_le_bytes());
        bytes.extend_from_slice(&2.0f64.to_le_bytes());
        bytes.extend_from_slice(&3.0f64.to_le_bytes());

        let error =
            Decoder::<Geometry<GeoGeometry<f64>>>::decode(&codec, &[Some(Bytes::from(bytes))])
                .expect_err("3D point should fail");
        assert!(error.to_string().contains("only 2D EWKB is supported"));
    }

    fn roundtrip<C, T>(codec: &C, value: &T) -> T
    where
        C: Encoder<T> + Decoder<T>,
        T: Clone + PartialEq + std::fmt::Debug,
    {
        let mut params = Vec::new();
        codec.encode(value, &mut params).expect("encode");
        let bytes = params
            .pop()
            .expect("slot")
            .expect("non-null parameter bytes");
        codec.decode(&[Some(Bytes::from(bytes))]).expect("decode")
    }
}
