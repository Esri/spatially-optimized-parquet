//! Encodes quantized geometry as nested Arrow arrays with integer or floating-point coordinates.
//!
//! Quantized native geometry keeps absolute integer coordinates in nested Arrow lists and
//! structs. Multipoints use `list<struct<x, y, z?, m?>>`, while polylines and polygons add an
//! outer list for parts. x and y remain non-null. z and m preserve their source validity through
//! nullable integer fields.

use std::marker::PhantomData;
use std::sync::Arc;

use arrow_array::ArrayRef;
use arrow_array::builder::{Float64Builder, Int64Builder, ListBuilder, StructBuilder};
use arrow_array::types::{Float64Type, Int64Type};
use arrow_schema::{DataType, Field, Fields};

use super::{
  ComponentValidity, CoordinateSpace, GeometryFamily, QuantizationTransform, QuantizedGeometry,
};

type CoordinateBuilder = StructBuilder;
type PointListBuilder = ListBuilder<CoordinateBuilder>;
type PartListBuilder = ListBuilder<CoordinateBuilder>;
type MultipartBuilder = ListBuilder<PartListBuilder>;

/// Defines Arrow leaf behavior for integer and floating-point native coordinate storage.
pub(crate) trait NativeCoordinateType {
  /// Return the Arrow type used for one coordinate component.
  fn data_type() -> DataType;

  /// Append one coordinate through this storage representation.
  fn append(
    builder: &mut CoordinateBuilder,
    field_index: usize,
    value: i64,
    coordinate_space: CoordinateSpace,
    transform: &QuantizationTransform,
    axis: usize,
  );

  /// Append a null optional coordinate component.
  fn append_null(builder: &mut CoordinateBuilder, field_index: usize);
}

impl NativeCoordinateType for Int64Type {
  fn data_type() -> DataType {
    DataType::Int64
  }

  fn append(
    builder: &mut CoordinateBuilder,
    field_index: usize,
    value: i64,
    _: CoordinateSpace,
    _: &QuantizationTransform,
    _: usize,
  ) {
    builder
      .field_builder::<Int64Builder>(field_index)
      .expect("integer coordinate builder")
      .append_value(value);
  }

  fn append_null(builder: &mut CoordinateBuilder, field_index: usize) {
    builder
      .field_builder::<Int64Builder>(field_index)
      .expect("integer coordinate builder")
      .append_null();
  }
}

impl NativeCoordinateType for Float64Type {
  fn data_type() -> DataType {
    DataType::Float64
  }

  fn append(
    builder: &mut CoordinateBuilder,
    field_index: usize,
    value: i64,
    coordinate_space: CoordinateSpace,
    transform: &QuantizationTransform,
    axis: usize,
  ) {
    builder
      .field_builder::<Float64Builder>(field_index)
      .expect("floating-point coordinate builder")
      .append_value(coordinate_space.decode(value, transform, axis));
  }

  fn append_null(builder: &mut CoordinateBuilder, field_index: usize) {
    builder
      .field_builder::<Float64Builder>(field_index)
      .expect("floating-point coordinate builder")
      .append_null();
  }
}

pub(crate) enum NativeGeometryArrayBuilder<T: NativeCoordinateType> {
  MultiPoint(PointListBuilder, CoordinateSpace, PhantomData<T>),
  Multipart(MultipartBuilder, CoordinateSpace, PhantomData<T>),
}

impl<T: NativeCoordinateType> NativeGeometryArrayBuilder<T> {
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
    coordinate_space: CoordinateSpace,
  ) -> Self {
    let coordinate_type = Self::coordinate_data_type(has_z, has_m);
    let coordinate_builder =
      StructBuilder::from_fields(Self::coordinate_fields(has_z, has_m), capacity);
    let point_builder = ListBuilder::with_capacity(coordinate_builder, capacity).with_field(
      Arc::new(Field::new("element", coordinate_type.clone(), false)),
    );
    match geometry_family {
      GeometryFamily::MultiPoint => Self::MultiPoint(point_builder, coordinate_space, PhantomData),
      GeometryFamily::Polyline | GeometryFamily::Polygon => {
        let part_type = DataType::List(Arc::new(Field::new("element", coordinate_type, false)));
        Self::Multipart(
          ListBuilder::with_capacity(point_builder, capacity)
            .with_field(Arc::new(Field::new("element", part_type, false))),
          coordinate_space,
          PhantomData,
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
      Self::MultiPoint(builder, coordinate_space, _) => {
        Self::append_coordinates(
          builder.values(),
          coordinates,
          has_z,
          has_m,
          validity,
          *coordinate_space,
          transform,
          0,
        );
        builder.append(true);
      }

      Self::Multipart(builder, coordinate_space, _) => {
        let mut coordinate_offset = 0usize;
        for &length in lengths {
          let value_count = length as usize * stride;
          Self::append_coordinates(
            builder.values().values(),
            &coordinates[coordinate_offset..coordinate_offset + value_count],
            has_z,
            has_m,
            validity,
            *coordinate_space,
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
      Arc::new(Field::new("x", T::data_type(), false)),
      Arc::new(Field::new("y", T::data_type(), false)),
    ];
    if has_z {
      fields.push(Arc::new(Field::new("z", T::data_type(), true)));
    }
    if has_m {
      fields.push(Arc::new(Field::new("m", T::data_type(), true)));
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
      Self::MultiPoint(builder, _, _) => builder.append(false),
      Self::Multipart(builder, _, _) => builder.append(false),
    }
  }

  pub(crate) fn finish(&mut self) -> ArrayRef {
    match self {
      Self::MultiPoint(builder, _, _) => Arc::new(builder.finish()),
      Self::Multipart(builder, _, _) => Arc::new(builder.finish()),
    }
  }

  fn append_coordinates(
    builder: &mut CoordinateBuilder,
    coordinates: &[i64],
    has_z: bool,
    has_m: bool,
    validity: &ComponentValidity,
    coordinate_space: CoordinateSpace,
    transform: &QuantizationTransform,
    coordinate_index_offset: usize,
  ) {
    let stride = coordinate_stride(has_z, has_m);
    for (local_coordinate_index, coordinate) in coordinates.chunks_exact(stride).enumerate() {
      let coordinate_index = coordinate_index_offset + local_coordinate_index;
      T::append(builder, 0, coordinate[0], coordinate_space, transform, 0);
      T::append(builder, 1, coordinate[1], coordinate_space, transform, 1);
      let mut component_index = 2;
      if has_z {
        if validity.z_is_valid(coordinate_index) {
          T::append(
            builder,
            component_index,
            coordinate[component_index],
            coordinate_space,
            transform,
            2,
          );
        } else {
          T::append_null(builder, component_index);
        }
        component_index += 1;
      }
      if has_m {
        if validity.m_is_valid(coordinate_index) {
          T::append(
            builder,
            component_index,
            coordinate[component_index],
            coordinate_space,
            transform,
            3,
          );
        } else {
          T::append_null(builder, component_index);
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
  use crate::geometry::GeometryFamily;

  #[test]
  fn builds_multipart_geometry_with_absolute_integer_coordinates() {
    let mut builder = NativeGeometryArrayBuilder::<Int64Type>::new(
      GeometryFamily::Polygon,
      false,
      false,
      1,
      CoordinateSpace::Quantized,
    );
    builder.append_geometry(
      &[1, 2, 4, 6, 7, 8],
      &[2, 1],
      false,
      false,
      &ComponentValidity::default(),
      &QuantizationTransform {
        scale: [1.0; 4],
        translate: [0.0; 4],
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
      .downcast_ref::<Int64Array>()
      .unwrap();

    assert_eq!(x.values(), &[1, 4]);
    assert_eq!(parts.len(), 2);
  }
}
