// Copyright 2026 Esri
//
// Licensed under the Apache License Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Encodes simplified geometry as nested Arrow arrays with floating-point coordinates.
//!
//! Native geometry stores snapped world coordinates in nested Arrow lists and structs.
//! Multipoints use `list<struct<x, y, z?, m?>>`, while polylines and polygons add an outer list
//! for parts. All coordinate fields remain non-null, with missing Z/M values encoded as zero.

use std::sync::Arc;

use arrow_array::ArrayRef;
use arrow_array::builder::{Float64Builder, ListBuilder, StructBuilder};
use arrow_schema::{DataType, Field, Fields};

use super::{GeometryFamily, QuantizationTransform, QuantizedGeometry};

type CoordinateListBuilder = ListBuilder<StructBuilder>;
type MultipartBuilder = ListBuilder<CoordinateListBuilder>;

/// Owns nested Arrow builders for multipoint or multipart geometry rows.
pub(crate) enum NativeGeometryArrayBuilder {
  MultiPoint(CoordinateListBuilder),
  Multipart(MultipartBuilder),
}

impl NativeGeometryArrayBuilder {
  /// Build reusable nested Arrow storage for the geometry family and coordinate layout.
  pub(crate) fn new(
    geometry_family: GeometryFamily,
    has_z: bool,
    has_m: bool,
    capacity: usize,
  ) -> Self {
    let fields = Self::coordinate_fields(has_z, has_m);
    let coordinate_type = DataType::Struct(fields.clone());
    let coordinate_builder = StructBuilder::from_fields(fields, capacity);
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

  /// Build the nested Arrow type for the geometry family and selected Z/M components.
  pub(crate) fn data_type(geometry_family: GeometryFamily, has_z: bool, has_m: bool) -> DataType {
    let coordinate_type = DataType::Struct(Self::coordinate_fields(has_z, has_m));
    let coordinate_list = DataType::List(Arc::new(Field::new("element", coordinate_type, false)));
    match geometry_family {
      GeometryFamily::MultiPoint => coordinate_list,
      GeometryFamily::Polyline | GeometryFamily::Polygon => {
        DataType::List(Arc::new(Field::new("element", coordinate_list, false)))
      }
      GeometryFamily::Point => unreachable!("points do not use multiscale geometry"),
    }
  }

  /// Resolve leaf column paths for each level's XY and optional Z/M components.
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

  /// Append a quantized geometry as world-space coordinates with its original part boundaries.
  pub(crate) fn append_quantized_geometry(
    &mut self,
    geometry: &QuantizedGeometry,
    transform: &QuantizationTransform,
  ) {
    match self {
      Self::MultiPoint(builder) => {
        Self::append_coordinates(
          builder.values(),
          &geometry.coordinates,
          geometry.has_z,
          geometry.has_m,
          transform,
        );
        builder.append(true);
      }

      Self::Multipart(builder) => {
        let stride = 2 + usize::from(geometry.has_z) + usize::from(geometry.has_m);
        let mut coordinate_offset = 0;
        for &length in &geometry.lengths {
          let value_count = length as usize * stride;
          Self::append_coordinates(
            builder.values().values(),
            &geometry.coordinates[coordinate_offset..coordinate_offset + value_count],
            geometry.has_z,
            geometry.has_m,
            transform,
          );
          builder.values().append(true);
          coordinate_offset += value_count;
        }
        builder.append(true);
      }
    }
  }

  /// Append a null geometry without adding coordinate or part values.
  pub(crate) fn append_null(&mut self) {
    match self {
      Self::MultiPoint(builder) => builder.append(false),
      Self::Multipart(builder) => builder.append(false),
    }
  }

  /// Finish the nested Arrow array and reset its builders for reuse.
  pub(crate) fn finish(&mut self) -> ArrayRef {
    match self {
      Self::MultiPoint(builder) => Arc::new(builder.finish()),
      Self::Multipart(builder) => Arc::new(builder.finish()),
    }
  }

  /// Build non-null XY fields followed by the selected Z/M fields.
  fn coordinate_fields(has_z: bool, has_m: bool) -> Fields {
    let mut fields = vec![
      Arc::new(Field::new("x", DataType::Float64, false)),
      Arc::new(Field::new("y", DataType::Float64, false)),
    ];
    if has_z {
      fields.push(Arc::new(Field::new("z", DataType::Float64, false)));
    }
    if has_m {
      fields.push(Arc::new(Field::new("m", DataType::Float64, false)));
    }
    Fields::from(fields)
  }

  /// Append absolute grid coordinates as world-space XY and optional Z/M struct fields.
  fn append_coordinates(
    builder: &mut StructBuilder,
    coordinates: &[i64],
    has_z: bool,
    has_m: bool,
    transform: &QuantizationTransform,
  ) {
    let stride = 2 + usize::from(has_z) + usize::from(has_m);
    for coordinate in coordinates.chunks_exact(stride) {
      append_value(builder, 0, coordinate[0], transform, 0);
      append_value(builder, 1, coordinate[1], transform, 1);
      let mut component_index = 2;
      if has_z {
        append_value(
          builder,
          component_index,
          coordinate[component_index],
          transform,
          2,
        );
        component_index += 1;
      }
      if has_m {
        append_value(
          builder,
          component_index,
          coordinate[component_index],
          transform,
          3,
        );
      }
      builder.append(true);
    }
  }
}

/// Append one integer component through its axis-specific scale and translation.
fn append_value(
  builder: &mut StructBuilder,
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

#[cfg(test)]
mod tests {
  use arrow_array::{Array, Float64Array, ListArray, StructArray};

  use super::*;
  use crate::geometry::{GeometryFamily, GeometryType};

  mod encode {
    use super::*;

    #[test]
    fn builds_multipart_geometry_with_world_coordinates() {
      let mut builder = NativeGeometryArrayBuilder::new(GeometryFamily::Polygon, false, false, 1);
      builder.append_quantized_geometry(
        &QuantizedGeometry {
          ty: crate::geometry::GeometryType::Polygon,
          coordinates: vec![1, 2, 4, 6, 7, 8],
          lengths: vec![2, 1],
          polygon_ring_counts: vec![2],
          has_z: false,
          has_m: false,
        },
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

    #[test]
    fn preserves_xy_z_m_and_null_rows_across_builder_reuse() {
      let transform = QuantizationTransform {
        scale: [0.5, 2.0, 3.0, 4.0],
        translate: [10.0, 20.0, 30.0, 40.0],
      };
      for (has_z, has_m) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut coordinate = vec![2, 3];
        if has_z {
          coordinate.push(4);
        }
        if has_m {
          coordinate.push(5);
        }
        for family in [
          GeometryFamily::MultiPoint,
          GeometryFamily::Polyline,
          GeometryFamily::Polygon,
        ] {
          let geometry = QuantizedGeometry {
            ty: match family {
              GeometryFamily::MultiPoint => GeometryType::MultiPoint,
              GeometryFamily::Polyline => GeometryType::MultiLineString,
              GeometryFamily::Polygon => GeometryType::Polygon,
              GeometryFamily::Point => unreachable!(),
            },
            coordinates: coordinate.repeat(3),
            lengths: if family == GeometryFamily::MultiPoint {
              vec![3]
            } else {
              vec![2, 0, 1]
            },
            polygon_ring_counts: if family == GeometryFamily::Polygon {
              vec![3]
            } else {
              vec![]
            },
            has_z,
            has_m,
          };
          let mut builder = NativeGeometryArrayBuilder::new(family, has_z, has_m, 3);
          for _ in 0..2 {
            builder.append_quantized_geometry(&geometry, &transform);
            builder.append_null();
            builder.append_quantized_geometry(&geometry, &transform);
            let array = builder.finish();
            assert_eq!(
              array.data_type(),
              &NativeGeometryArrayBuilder::data_type(family, has_z, has_m)
            );
            let rows = array.as_any().downcast_ref::<ListArray>().unwrap();
            assert_eq!(rows.len(), 3);
            assert!(rows.is_null(1));
            for row_index in [0, 2] {
              let values = rows.value(row_index);
              let coordinates = if family == GeometryFamily::MultiPoint {
                assert_eq!(values.len(), 3);
                values
              } else {
                let parts = values.as_any().downcast_ref::<ListArray>().unwrap();
                assert_eq!(
                  [
                    parts.value_length(0),
                    parts.value_length(1),
                    parts.value_length(2)
                  ],
                  [2, 0, 1]
                );
                parts.value(0)
              };
              let coordinates = coordinates.as_any().downcast_ref::<StructArray>().unwrap();
              assert_eq!(coordinates.num_columns(), coordinate.len());
              for (name, expected) in [
                ("x", Some(11.0)),
                ("y", Some(26.0)),
                ("z", has_z.then_some(42.0)),
                ("m", has_m.then_some(60.0)),
              ] {
                let column = coordinates.column_by_name(name);
                if let Some(expected) = expected {
                  let column = column
                    .unwrap()
                    .as_any()
                    .downcast_ref::<Float64Array>()
                    .unwrap();
                  assert_eq!(column.null_count(), 0);
                  assert!(column.values().iter().all(|value| *value == expected));
                } else {
                  assert!(column.is_none());
                }
              }
            }
          }
          assert_eq!(geometry.coordinates, coordinate.repeat(3));
        }
      }
    }
  }

  mod schema {
    use super::*;

    #[test]
    fn resolves_leaf_paths_for_each_family_and_layout() {
      for (family, nesting) in [
        (GeometryFamily::MultiPoint, "list.element"),
        (GeometryFamily::Polyline, "list.element.list.element"),
        (GeometryFamily::Polygon, "list.element.list.element"),
      ] {
        for (has_z, has_m) in [(false, false), (true, false), (false, true), (true, true)] {
          let paths = NativeGeometryArrayBuilder::coordinate_column_paths(
            "geometry",
            &["level_0".to_string(), "level_1".to_string()],
            family,
            has_z,
            has_m,
          );
          let mut components = vec!["x", "y"];
          if has_z {
            components.push("z");
          }
          if has_m {
            components.push("m");
          }
          let expected: Vec<_> = ["level_0", "level_1"]
            .into_iter()
            .flat_map(|level| {
              components
                .iter()
                .map(move |component| format!("geometry.{level}.{nesting}.{component}"))
            })
            .collect();

          assert_eq!(paths, expected);
        }
      }
      assert!(
        NativeGeometryArrayBuilder::coordinate_column_paths(
          "geometry",
          &["level_0".to_string()],
          GeometryFamily::Point,
          true,
          true,
        )
        .is_empty()
      );
    }
  }
}
