//! Encodes quantized geometry as nested Arrow arrays with integer coordinates.
//!
//! Quantized native geometry keeps absolute integer coordinates in nested Arrow lists and
//! structs. Multipoints use `list<struct<x, y, z?, m?>>`, while polylines and polygons add an
//! outer list for parts. x and y remain non-null. z and m preserve their source validity through
//! nullable integer fields.

use std::sync::Arc;

use arrow_array::ArrayRef;
use arrow_array::builder::{Int64Builder, ListBuilder, StructBuilder};
use arrow_schema::{DataType, Field, Fields};

use super::{ComponentValidity, GeometryType, QuantizedGeometry};

type CoordinateBuilder = StructBuilder;
type PointListBuilder = ListBuilder<CoordinateBuilder>;
type PartListBuilder = ListBuilder<CoordinateBuilder>;
type MultipartBuilder = ListBuilder<PartListBuilder>;

pub(crate) enum NativeGeometryArrayBuilder {
  MultiPoint(PointListBuilder),
  Multipart(MultipartBuilder),
}

impl NativeGeometryArrayBuilder {
  pub(crate) fn data_type(geometry_type: GeometryType, has_z: bool, has_m: bool) -> DataType {
    let coordinate_type = Self::coordinate_data_type(has_z, has_m);
    let coordinate_list = DataType::List(Arc::new(Field::new("element", coordinate_type, false)));
    match geometry_type {
      GeometryType::MultiPoint => coordinate_list,
      GeometryType::Polyline | GeometryType::Polygon => {
        DataType::List(Arc::new(Field::new("element", coordinate_list, false)))
      }
      GeometryType::Point => unreachable!("points do not use multiscale geometry"),
    }
  }

  pub(crate) fn coordinate_column_paths(
    parent_column: &str,
    level_columns: &[String],
    geometry_type: GeometryType,
    has_z: bool,
    has_m: bool,
  ) -> Vec<String> {
    let nesting = match geometry_type {
      GeometryType::MultiPoint => "list.element",
      GeometryType::Polyline | GeometryType::Polygon => "list.element.list.element",
      GeometryType::Point => return Vec::new(),
    };
    let mut components = vec!["x", "y"];
    if has_z {
      components.push("z");
    }
    if has_m {
      components.push("m");
    }
    level_columns
      .iter()
      .flat_map(|level_column| {
        components
          .iter()
          .map(move |component| format!("{parent_column}.{level_column}.{nesting}.{component}"))
      })
      .collect()
  }

  pub(crate) fn new(
    geometry_type: GeometryType,
    has_z: bool,
    has_m: bool,
    capacity: usize,
  ) -> Self {
    let coordinate_type = Self::coordinate_data_type(has_z, has_m);
    let coordinate_builder =
      StructBuilder::from_fields(Self::coordinate_fields(has_z, has_m), capacity);
    let point_builder = ListBuilder::with_capacity(coordinate_builder, capacity).with_field(
      Arc::new(Field::new("element", coordinate_type.clone(), false)),
    );
    match geometry_type {
      GeometryType::MultiPoint => Self::MultiPoint(point_builder),
      GeometryType::Polyline | GeometryType::Polygon => {
        let part_type = DataType::List(Arc::new(Field::new("element", coordinate_type, false)));
        Self::Multipart(
          ListBuilder::with_capacity(point_builder, capacity)
            .with_field(Arc::new(Field::new("element", part_type, false))),
        )
      }
      GeometryType::Point => unreachable!("points do not use multiscale geometry"),
    }
  }

  fn append_geometry(
    &mut self,
    coordinates: &[i64],
    lengths: &[u32],
    has_z: bool,
    has_m: bool,
    validity: &ComponentValidity,
  ) {
    let stride = coordinate_stride(has_z, has_m);
    match self {
      Self::MultiPoint(builder) => {
        Self::append_coordinates(builder.values(), coordinates, has_z, has_m, validity, 0);
        builder.append(true);
      }

      Self::Multipart(builder) => {
        let mut coordinate_offset = 0usize;
        for &length in lengths {
          let value_count = length as usize * stride;
          Self::append_coordinates(
            builder.values().values(),
            &coordinates[coordinate_offset..coordinate_offset + value_count],
            has_z,
            has_m,
            validity,
            coordinate_offset / stride,
          );
          builder.values().append(true);
          coordinate_offset += value_count;
        }
        builder.append(true);
      }
    }
  }

  fn coordinate_data_type(has_z: bool, has_m: bool) -> DataType {
    DataType::Struct(Self::coordinate_fields(has_z, has_m))
  }

  fn coordinate_fields(has_z: bool, has_m: bool) -> Fields {
    let mut fields = vec![
      Arc::new(Field::new("x", DataType::Int64, false)),
      Arc::new(Field::new("y", DataType::Int64, false)),
    ];
    if has_z {
      fields.push(Arc::new(Field::new("z", DataType::Int64, true)));
    }
    if has_m {
      fields.push(Arc::new(Field::new("m", DataType::Int64, true)));
    }
    Fields::from(fields)
  }

  pub(crate) fn append_quantized_geometry(&mut self, geometry: &QuantizedGeometry) {
    self.append_geometry(
      &geometry.coordinates,
      &geometry.lengths,
      geometry.has_z,
      geometry.has_m,
      &geometry.validity,
    );
  }

  pub(crate) fn append_null(&mut self) {
    match self {
      Self::MultiPoint(builder) => builder.append(false),
      Self::Multipart(builder) => builder.append(false),
    }
  }

  pub(crate) fn finish(&mut self) -> ArrayRef {
    match self {
      Self::MultiPoint(builder) => Arc::new(builder.finish()),
      Self::Multipart(builder) => Arc::new(builder.finish()),
    }
  }

  fn append_coordinates(
    builder: &mut CoordinateBuilder,
    coordinates: &[i64],
    has_z: bool,
    has_m: bool,
    validity: &ComponentValidity,
    coordinate_index_offset: usize,
  ) {
    let stride = coordinate_stride(has_z, has_m);
    for (local_coordinate_index, coordinate) in coordinates.chunks_exact(stride).enumerate() {
      let coordinate_index = coordinate_index_offset + local_coordinate_index;
      builder
        .field_builder::<Int64Builder>(0)
        .expect("x coordinate builder")
        .append_value(coordinate[0]);
      builder
        .field_builder::<Int64Builder>(1)
        .expect("y coordinate builder")
        .append_value(coordinate[1]);
      let mut component_index = 2;
      if has_z {
        let builder = builder
          .field_builder::<Int64Builder>(component_index)
          .expect("z coordinate builder");
        if validity.z_is_valid(coordinate_index) {
          builder.append_value(coordinate[component_index]);
        } else {
          builder.append_null();
        }
        component_index += 1;
      }
      if has_m {
        let builder = builder
          .field_builder::<Int64Builder>(component_index)
          .expect("m coordinate builder");
        if validity.m_is_valid(coordinate_index) {
          builder.append_value(coordinate[component_index]);
        } else {
          builder.append_null();
        }
      }
      builder.append(true);
    }
  }
}

fn coordinate_stride(has_z: bool, has_m: bool) -> usize {
  2 + usize::from(has_z) + usize::from(has_m)
}

#[cfg(test)]
mod tests {
  use arrow_array::{Array, Int64Array, ListArray, StructArray};

  use super::*;

  #[test]
  fn builds_multipart_geometry_with_absolute_integer_coordinates() {
    let mut builder = NativeGeometryArrayBuilder::new(GeometryType::Polygon, false, false, 1);
    builder.append_geometry(
      &[1, 2, 4, 6, 7, 8],
      &[2, 1],
      false,
      false,
      &ComponentValidity::default(),
    );
    let array = builder.finish();
    let geometries = array.as_any().downcast_ref::<ListArray>().unwrap();
    let parts = geometries.value(0);
    let parts = parts.as_any().downcast_ref::<ListArray>().unwrap();
    let coordinates = parts.value(0);
    let coordinates = coordinates.as_any().downcast_ref::<StructArray>().unwrap();
    let x = coordinates
      .column_by_name("x")
      .unwrap()
      .as_any()
      .downcast_ref::<Int64Array>()
      .unwrap();

    assert_eq!(x.values(), &[1, 4]);
    assert_eq!(parts.len(), 2);
  }
}
