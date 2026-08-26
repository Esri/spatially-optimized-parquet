//! Encodes simplified geometry as nested Arrow arrays with floating-point coordinates.
//!
//! Native geometry stores snapped world coordinates in nested Arrow lists and structs.
//! Multipoints use `list<struct<x, y, z?, m?>>`, while polylines and polygons add an outer list
//! for parts. x and y remain non-null. z and m preserve their source validity through nullable
//! floating-point fields.

use std::sync::Arc;

use arrow_array::ArrayRef;
use arrow_array::builder::{Float64Builder, ListBuilder, StructBuilder};
use arrow_schema::{DataType, Field, Fields};

use super::{ComponentValidity, GeometryFamily, QuantizationTransform, QuantizedGeometry};

type CoordinateBuilder = StructBuilder;
type PointListBuilder = ListBuilder<CoordinateBuilder>;
type PartListBuilder = ListBuilder<CoordinateBuilder>;
type MultipartBuilder = ListBuilder<PartListBuilder>;

pub(crate) enum NativeGeometryArrayBuilder {
  MultiPoint(PointListBuilder),
  Multipart(MultipartBuilder),
}

impl NativeGeometryArrayBuilder {
  pub(crate) fn data_type(geometry_family: GeometryFamily, has_z: bool, has_m: bool) -> DataType {
    let coordinate_type = Self::coordinate_data_type(has_z, has_m);
    let coordinate_list = DataType::List(Arc::new(Field::new("element", coordinate_type, false)));
    match geometry_family {
      GeometryFamily::MultiPoint => coordinate_list,
      GeometryFamily::Polyline | GeometryFamily::Polygon => {
        DataType::List(Arc::new(Field::new("element", coordinate_list, false)))
      }
      GeometryFamily::Point => unreachable!("points do not use multiscale geometry"),
    }
  }

  pub(crate) fn coordinate_column_paths(
    parent_column: &str,
    level_columns: &[String],
    geometry_family: GeometryFamily,
    has_z: bool,
    has_m: bool,
  ) -> Vec<String> {
    let nesting = match geometry_family {
      GeometryFamily::MultiPoint => "list.element",
      GeometryFamily::Polyline | GeometryFamily::Polygon => "list.element.list.element",
      GeometryFamily::Point => return Vec::new(),
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
    geometry_family: GeometryFamily,
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
    match geometry_family {
      GeometryFamily::MultiPoint => Self::MultiPoint(point_builder),
      GeometryFamily::Polyline | GeometryFamily::Polygon => {
        let part_type = DataType::List(Arc::new(Field::new("element", coordinate_type, false)));
        Self::Multipart(
          ListBuilder::with_capacity(point_builder, capacity)
            .with_field(Arc::new(Field::new("element", part_type, false))),
        )
      }
      GeometryFamily::Point => unreachable!("points do not use multiscale geometry"),
    }
  }

  fn append_geometry(
    &mut self,
    coordinates: &[i64],
    lengths: &[u32],
    has_z: bool,
    has_m: bool,
    validity: &ComponentValidity,
    transform: &QuantizationTransform,
  ) {
    let stride = coordinate_stride(has_z, has_m);
    match self {
      Self::MultiPoint(builder) => {
        Self::append_coordinates(
          builder.values(),
          coordinates,
          has_z,
          has_m,
          validity,
          transform,
          0,
        );
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
            transform,
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
      Arc::new(Field::new("x", DataType::Float64, false)),
      Arc::new(Field::new("y", DataType::Float64, false)),
    ];
    if has_z {
      fields.push(Arc::new(Field::new("z", DataType::Float64, true)));
    }
    if has_m {
      fields.push(Arc::new(Field::new("m", DataType::Float64, true)));
    }
    Fields::from(fields)
  }

  pub(crate) fn append_quantized_geometry(
    &mut self,
    geometry: &QuantizedGeometry,
    transform: &QuantizationTransform,
  ) {
    self.append_geometry(
      &geometry.coordinates,
      &geometry.lengths,
      geometry.has_z,
      geometry.has_m,
      &geometry.validity,
      transform,
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
    transform: &QuantizationTransform,
    coordinate_index_offset: usize,
  ) {
    let stride = coordinate_stride(has_z, has_m);
    for (local_coordinate_index, coordinate) in coordinates.chunks_exact(stride).enumerate() {
      let coordinate_index = coordinate_index_offset + local_coordinate_index;
      append_value(builder, 0, coordinate[0], transform, 0);
      append_value(builder, 1, coordinate[1], transform, 1);
      let mut component_index = 2;
      if has_z {
        if validity.z_is_valid(coordinate_index) {
          append_value(
            builder,
            component_index,
            coordinate[component_index],
            transform,
            2,
          );
        } else {
          append_null(builder, component_index);
        }
        component_index += 1;
      }
      if has_m {
        if validity.m_is_valid(coordinate_index) {
          append_value(
            builder,
            component_index,
            coordinate[component_index],
            transform,
            3,
          );
        } else {
          append_null(builder, component_index);
        }
      }
      builder.append(true);
    }
  }
}

fn append_value(
  builder: &mut CoordinateBuilder,
  field_index: usize,
  value: i64,
  transform: &QuantizationTransform,
  axis: usize,
) {
  builder
    .field_builder::<Float64Builder>(field_index)
    .expect("floating-point coordinate builder")
    .append_value(transform.unquantize(value, axis));
}

fn append_null(builder: &mut CoordinateBuilder, field_index: usize) {
  builder
    .field_builder::<Float64Builder>(field_index)
    .expect("floating-point coordinate builder")
    .append_null();
}

fn coordinate_stride(has_z: bool, has_m: bool) -> usize {
  2 + usize::from(has_z) + usize::from(has_m)
}

#[cfg(test)]
mod tests {
  use arrow_array::{Array, Float64Array, ListArray, StructArray};

  use super::*;
  use crate::geometry::GeometryFamily;

  #[test]
  fn builds_multipart_geometry_with_world_coordinates() {
    let mut builder = NativeGeometryArrayBuilder::new(GeometryFamily::Polygon, false, false, 1);
    builder.append_geometry(
      &[1, 2, 4, 6, 7, 8],
      &[2, 1],
      false,
      false,
      &ComponentValidity::default(),
      &QuantizationTransform {
        scale: [0.5; 4],
        translate: [10.0; 4],
      },
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
      .downcast_ref::<Float64Array>()
      .unwrap();

    assert_eq!(x.values(), &[10.5, 12.0]);
    assert_eq!(parts.len(), 2);
  }
}
