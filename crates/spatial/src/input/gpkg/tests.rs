use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use arrow_array::{Int32Array, StringArray, StringViewArray};
use datafusion::common::tree_node::{TreeNode, TreeNodeRecursion};
use datafusion::physical_plan::ExecutionPlanProperties;
use futures_util::StreamExt;
use gdal::spatial_ref::SpatialRef;
use gdal::vector::{Feature, Geometry, LayerAccess, LayerOptions};
use gdal::{Dataset, DriverManager};
use gdal_sys::{OGRFieldType, OGRwkbGeometryType};
use tempfile::TempDir;
use tokio::runtime::Runtime;

use crate::geometry::{
  CoordinateDimensions, Extent2D, GeometryArray, GeometryKind, PolygonRingOrder, WkbCoordinate,
  WkbPartRole, WkbSink, visit_wkb_geometry,
};
use crate::input::{InputOpenOptions, InputSource, RowRange, SourceFormat, open_input};
use crate::session::DataFusionSession;
use crate::{InputOptions, OutputMode, OutputOptions, Pipeline, SpatialPipelineOptions, validate};

fn runtime() -> Runtime {
  Runtime::new().unwrap()
}

struct GpkgFeature<'a> {
  id: i32,
  name: Option<&'a str>,
  geometry_wkt: &'a str,
}

struct GpkgLayer<'a> {
  name: &'a str,
  geometry_type: OGRwkbGeometryType::Type,
  epsg: Option<u32>,
  features: &'a [GpkgFeature<'a>],
}

fn write_gpkg(path: &Path, layers: &[GpkgLayer<'_>]) {
  if path.exists() {
    std::fs::remove_file(path).unwrap();
  }
  let driver = DriverManager::get_driver_by_name("GPKG")
    .expect("GDAL GPKG driver is required for GeoPackage tests");
  let mut dataset = driver.create_vector_only(path).unwrap();
  for layer_spec in layers {
    let spatial_ref = layer_spec
      .epsg
      .map(|epsg| SpatialRef::from_epsg(epsg).unwrap());
    let geometry_options = ["GEOMETRY_NAME=geometry"];
    let layer = dataset
      .create_layer(LayerOptions {
        name: layer_spec.name,
        srs: spatial_ref.as_ref(),
        ty: layer_spec.geometry_type,
        options: Some(&geometry_options),
      })
      .unwrap();
    layer
      .create_defn_fields(&[
        ("id", OGRFieldType::OFTInteger),
        ("name", OGRFieldType::OFTString),
      ])
      .unwrap();
    let id_index = layer.defn().field_index("id").unwrap();
    let name_index = layer.defn().field_index("name").unwrap();
    for feature_spec in layer_spec.features {
      let mut feature = Feature::new(layer.defn()).unwrap();
      feature
        .set_field_integer(id_index, feature_spec.id)
        .unwrap();
      if let Some(name) = feature_spec.name {
        feature.set_field_string(name_index, name).unwrap();
      }
      feature
        .set_geometry(Geometry::from_wkt(feature_spec.geometry_wkt).unwrap())
        .unwrap();
      feature.create(&layer).unwrap();
    }
  }
  dataset.flush_cache().unwrap();
}

fn open_gpkg_dataset(path: &Path) -> Dataset {
  Dataset::open(path).unwrap()
}

fn open_gpkg_input(path: &Path, layer: Option<&str>) -> Arc<dyn InputSource> {
  runtime()
    .block_on(open_input(
      SourceFormat::GeoPackage,
      &InputOpenOptions::new(
        path.to_string_lossy().into_owned(),
        layer.map(str::to_string),
      ),
    ))
    .unwrap()
}

fn string_value(array: &dyn arrow_array::Array, index: usize) -> String {
  if let Some(array) = array.as_any().downcast_ref::<StringArray>() {
    return array.value(index).to_string();
  }
  if let Some(array) = array.as_any().downcast_ref::<StringViewArray>() {
    return array.value(index).to_string();
  }
  panic!("unexpected string array type: {:?}", array.data_type());
}

fn binary_value(array: &dyn arrow_array::Array, index: usize) -> Vec<u8> {
  GeometryArray::try_new(array)
    .unwrap()
    .value(index)
    .unwrap()
    .to_vec()
}

#[derive(Default)]
struct CoordinateSink {
  coordinates: Vec<WkbCoordinate>,
}

impl WkbSink for CoordinateSink {
  fn start_part(&mut self, _role: WkbPartRole) {}

  fn push_coord(&mut self, coordinate: WkbCoordinate) {
    self.coordinates.push(coordinate);
  }

  fn finish_part(&mut self) {}
}

#[test]
fn open_input_accepts_single_layer_geopackage_and_reads_metadata() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("cities.gpkg");
  let features = [
    GpkgFeature {
      id: 2,
      name: Some("late"),
      geometry_wkt: "POINT (8 8)",
    },
    GpkgFeature {
      id: 1,
      name: Some("early"),
      geometry_wkt: "POINT (1 1)",
    },
  ];
  write_gpkg(
    &path,
    &[GpkgLayer {
      name: "cities",
      geometry_type: OGRwkbGeometryType::wkbPoint,
      epsg: Some(4326),
      features: &features,
    }],
  );

  let dataset = open_gpkg_dataset(&path);
  let layer = dataset.layer_by_name("cities").unwrap();
  assert_eq!(layer.feature_count(), 2);
  assert_eq!(
    layer.defn().geom_fields().next().unwrap().name(),
    "geometry"
  );

  let input = open_gpkg_input(&path, None);
  let session = DataFusionSession::new(None, None).unwrap();
  assert_eq!(input.total_rows().unwrap(), 2);
  let schema = input.schema().unwrap();
  assert!(schema.field_with_name("id").is_ok());
  assert!(schema.field_with_name("geometry").is_ok());

  let geometry_column = input.inferred_geometry_column().unwrap().unwrap();
  assert_eq!(geometry_column.column, "geometry");
  assert_eq!(geometry_column.geometry_kind, Some(GeometryKind::Point));

  let geometry_meta = input.source_metadata().unwrap().geometry.unwrap();
  assert_eq!(geometry_meta.geometry_types, vec![GeometryKind::Point]);
  assert_eq!(
    geometry_meta.bbox,
    Some(Extent2D {
      xmin: 1.0,
      ymin: 1.0,
      xmax: 8.0,
      ymax: 8.0,
    })
  );
  let projjson = geometry_meta.projjson.unwrap();
  assert_eq!(projjson["id"]["authority"], "EPSG");
  assert_eq!(projjson["id"]["code"], 4326);

  let (ids, names) = runtime().block_on(async {
    let dataframe = input
      .to_dataframe(session.context(), RowRange::default())
      .await
      .unwrap();
    let mut stream = dataframe.execute_stream().await.unwrap();
    let mut ids = Vec::new();
    let mut names = Vec::new();
    while let Some(batch) = stream.next().await {
      let batch = batch.unwrap();
      let id_array = batch
        .column_by_name("id")
        .unwrap()
        .as_any()
        .downcast_ref::<Int32Array>()
        .unwrap();
      let name_array = batch.column_by_name("name").unwrap();
      for index in 0..batch.num_rows() {
        ids.push(id_array.value(index));
        names.push(string_value(name_array.as_ref(), index));
      }
    }
    (ids, names)
  });

  let mut records = ids.into_iter().zip(names).collect::<Vec<_>>();
  records.sort_unstable_by_key(|(id, _)| *id);
  assert_eq!(
    records,
    vec![(1, "early".to_string()), (2, "late".to_string())]
  );
}

#[test]
fn open_input_requires_layer_for_multi_layer_geopackage() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("multi.gpkg");
  let point_features = [GpkgFeature {
    id: 1,
    name: Some("point"),
    geometry_wkt: "POINT (0 0)",
  }];
  let polygon_features = [GpkgFeature {
    id: 2,
    name: Some("polygon"),
    geometry_wkt: "POLYGON ((0 0, 1 0, 1 1, 0 0))",
  }];
  write_gpkg(
    &path,
    &[
      GpkgLayer {
        name: "points",
        geometry_type: OGRwkbGeometryType::wkbPoint,
        epsg: Some(4326),
        features: &point_features,
      },
      GpkgLayer {
        name: "polygons",
        geometry_type: OGRwkbGeometryType::wkbPolygon,
        epsg: Some(4326),
        features: &polygon_features,
      },
    ],
  );

  let err = match runtime().block_on(open_input(
    SourceFormat::GeoPackage,
    &InputOpenOptions::new(path.to_string_lossy().into_owned(), None),
  )) {
    Ok(_) => panic!("expected multi-layer GeoPackage without --layer to fail"),
    Err(err) => err,
  };
  let message = format!("{err:#}");
  assert!(message.contains("contains multiple layers"));
  assert!(message.contains("- points (geometry: Point, features: 1)"));
  assert!(message.contains("- polygons (geometry: Polygon, features: 1)"));
}

#[test]
fn open_input_selects_requested_geopackage_layer() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("multi.gpkg");
  let point_features = [GpkgFeature {
    id: 1,
    name: Some("point"),
    geometry_wkt: "POINT (0 0)",
  }];
  let polygon_features = [GpkgFeature {
    id: 2,
    name: Some("polygon"),
    geometry_wkt: "POLYGON ((0 0, 1 0, 1 1, 0 0))",
  }];
  write_gpkg(
    &path,
    &[
      GpkgLayer {
        name: "points",
        geometry_type: OGRwkbGeometryType::wkbPoint,
        epsg: Some(4326),
        features: &point_features,
      },
      GpkgLayer {
        name: "polygons",
        geometry_type: OGRwkbGeometryType::wkbPolygon,
        epsg: Some(4326),
        features: &polygon_features,
      },
    ],
  );

  let input = open_gpkg_input(&path, Some("polygons"));
  assert_eq!(input.total_rows().unwrap(), 1);
  let geometry_column = input.inferred_geometry_column().unwrap().unwrap();
  assert_eq!(geometry_column.column, "geometry");
  assert_eq!(geometry_column.geometry_kind, Some(GeometryKind::Polygon));
}

#[test]
fn geopackage_input_can_produce_dataframe_for_execution() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("cities.gpkg");
  let features = [
    GpkgFeature {
      id: 2,
      name: Some("late"),
      geometry_wkt: "POINT (8 8)",
    },
    GpkgFeature {
      id: 1,
      name: Some("early"),
      geometry_wkt: "POINT (1 1)",
    },
  ];
  write_gpkg(
    &path,
    &[GpkgLayer {
      name: "cities",
      geometry_type: OGRwkbGeometryType::wkbPoint,
      epsg: Some(4326),
      features: &features,
    }],
  );

  let input = open_gpkg_input(&path, None);
  let session = DataFusionSession::new(None, None).unwrap();
  let rows = runtime().block_on(async {
    let df = input
      .to_dataframe(session.context(), RowRange::new(0, Some(1)))
      .await
      .unwrap();
    let batches = df.collect().await.unwrap();
    batches.iter().map(|batch| batch.num_rows()).sum::<usize>()
  });
  assert_eq!(rows, 1);
}

#[test]
fn geopackage_dataframe_uses_partitioned_scan_for_full_reads() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("partitioned-full.gpkg");
  let features = [
    GpkgFeature {
      id: 1,
      name: Some("a"),
      geometry_wkt: "POINT (0 0)",
    },
    GpkgFeature {
      id: 2,
      name: Some("b"),
      geometry_wkt: "POINT (1 1)",
    },
    GpkgFeature {
      id: 3,
      name: Some("c"),
      geometry_wkt: "POINT (2 2)",
    },
    GpkgFeature {
      id: 4,
      name: Some("d"),
      geometry_wkt: "POINT (3 3)",
    },
  ];
  write_gpkg(
    &path,
    &[GpkgLayer {
      name: "points",
      geometry_type: OGRwkbGeometryType::wkbPoint,
      epsg: Some(4326),
      features: &features,
    }],
  );

  let input = open_gpkg_input(&path, None);
  let session = DataFusionSession::new(None, None).unwrap();
  let (ids, saw_partitioned_node) = runtime().block_on(async {
    let df = input
      .to_dataframe(session.context(), RowRange::default())
      .await
      .unwrap();
    let physical_plan = df.clone().create_physical_plan().await.unwrap();
    let mut saw_partitioned_node = false;
    physical_plan
      .apply(|plan| {
        if plan.output_partitioning().partition_count() > 1 {
          saw_partitioned_node = true;
        }
        Ok(TreeNodeRecursion::Continue)
      })
      .unwrap();

    let batches = df.collect().await.unwrap();
    let ids = batches
      .iter()
      .flat_map(|batch| {
        let ids = batch
          .column_by_name("id")
          .unwrap()
          .as_any()
          .downcast_ref::<Int32Array>()
          .unwrap();
        (0..batch.num_rows())
          .map(|index| ids.value(index))
          .collect::<Vec<_>>()
      })
      .collect::<BTreeSet<_>>();
    (ids, saw_partitioned_node)
  });

  assert!(
    saw_partitioned_node,
    "expected a partitioned scan node in the plan"
  );
  assert_eq!(ids, BTreeSet::from([1, 2, 3, 4]));
}

#[test]
fn geopackage_dataframe_limit_uses_partitioned_scan_path() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("partitioned-limit.gpkg");
  let features = [
    GpkgFeature {
      id: 1,
      name: Some("a"),
      geometry_wkt: "POINT (0 0)",
    },
    GpkgFeature {
      id: 2,
      name: Some("b"),
      geometry_wkt: "POINT (1 1)",
    },
    GpkgFeature {
      id: 3,
      name: Some("c"),
      geometry_wkt: "POINT (2 2)",
    },
    GpkgFeature {
      id: 4,
      name: Some("d"),
      geometry_wkt: "POINT (3 3)",
    },
    GpkgFeature {
      id: 5,
      name: Some("e"),
      geometry_wkt: "POINT (4 4)",
    },
    GpkgFeature {
      id: 6,
      name: Some("f"),
      geometry_wkt: "POINT (5 5)",
    },
  ];
  write_gpkg(
    &path,
    &[GpkgLayer {
      name: "points",
      geometry_type: OGRwkbGeometryType::wkbPoint,
      epsg: Some(4326),
      features: &features,
    }],
  );

  let input = open_gpkg_input(&path, None);
  let session = DataFusionSession::new(None, None).unwrap();
  let (row_count, ids, saw_partitioned_node) = runtime().block_on(async {
    let df = input
      .to_dataframe(session.context(), RowRange::new(0, Some(3)))
      .await
      .unwrap();
    let physical_plan = df.clone().create_physical_plan().await.unwrap();
    let mut saw_partitioned_node = false;
    physical_plan
      .apply(|plan| {
        if plan.output_partitioning().partition_count() > 1 {
          saw_partitioned_node = true;
        }
        Ok(TreeNodeRecursion::Continue)
      })
      .unwrap();

    let batches = df.collect().await.unwrap();
    let ids = batches
      .iter()
      .flat_map(|batch| {
        let ids = batch
          .column_by_name("id")
          .unwrap()
          .as_any()
          .downcast_ref::<Int32Array>()
          .unwrap();
        (0..batch.num_rows())
          .map(|index| ids.value(index))
          .collect::<Vec<_>>()
      })
      .collect::<BTreeSet<_>>();
    (
      batches.iter().map(|batch| batch.num_rows()).sum::<usize>(),
      ids,
      saw_partitioned_node,
    )
  });

  assert!(
    saw_partitioned_node,
    "expected a partitioned scan node in the limited plan"
  );
  assert_eq!(row_count, 3);
  assert_eq!(ids.len(), 3);
  assert!(ids.iter().all(|id| (1..=6).contains(id)));
}

#[test]
fn open_input_uses_sampled_geometry_type_for_generic_layer_metadata() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("generic-multipolygon.gpkg");
  let generic_features = [GpkgFeature {
    id: 1,
    name: Some("generic"),
    geometry_wkt: "MULTIPOLYGON (((0 0, 2 0, 2 2, 0 0)))",
  }];
  write_gpkg(
    &path,
    &[GpkgLayer {
      name: "generic_spatial",
      geometry_type: OGRwkbGeometryType::wkbUnknown,
      epsg: Some(4326),
      features: &generic_features,
    }],
  );

  let input = open_gpkg_input(&path, None);
  let geometry_column = input.inferred_geometry_column().unwrap().unwrap();
  assert_eq!(
    geometry_column.geometry_kind,
    Some(GeometryKind::MultiPolygon)
  );

  let geometry_meta = input.source_metadata().unwrap().geometry.unwrap();
  assert_eq!(
    geometry_meta.geometry_types,
    vec![GeometryKind::MultiPolygon]
  );
  assert_eq!(
    geometry_meta.bbox,
    Some(Extent2D {
      xmin: 0.0,
      ymin: 0.0,
      xmax: 2.0,
      ymax: 2.0,
    })
  );
}

#[test]
fn open_input_infers_z_and_m_from_generic_geopackage_wkb() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("generic-point-zm.gpkg");
  let features = [GpkgFeature {
    id: 1,
    name: Some("dimensional"),
    geometry_wkt: "POINT ZM (1 2 3 4)",
  }];
  write_gpkg(
    &path,
    &[GpkgLayer {
      name: "generic_spatial",
      geometry_type: OGRwkbGeometryType::wkbUnknown,
      epsg: Some(4326),
      features: &features,
    }],
  );

  let input = open_gpkg_input(&path, None);
  let geometry = input.source_metadata().unwrap().geometry.unwrap();

  assert_eq!(geometry.geometry_types, vec![GeometryKind::Point]);
  assert!(geometry.has_z);
  assert!(geometry.has_m);
}

#[test]
fn geopackage_input_preserves_z_and_m_wkb_for_supported_geometry_types() {
  struct DimensionCase {
    suffix: &'static str,
    dimension_values: &'static str,
    has_z: bool,
    has_m: bool,
    dimensions: CoordinateDimensions,
  }

  struct GeometryCase {
    name: &'static str,
    geometry_kind: GeometryKind,
    geometry_types: [OGRwkbGeometryType::Type; 3],
    supports_optimized_output: bool,
  }

  let dimension_cases = [
    DimensionCase {
      suffix: "Z",
      dimension_values: "3",
      has_z: true,
      has_m: false,
      dimensions: CoordinateDimensions::Xyz,
    },
    DimensionCase {
      suffix: "M",
      dimension_values: "4",
      has_z: false,
      has_m: true,
      dimensions: CoordinateDimensions::Xym,
    },
    DimensionCase {
      suffix: "ZM",
      dimension_values: "3 4",
      has_z: true,
      has_m: true,
      dimensions: CoordinateDimensions::Xyzm,
    },
  ];
  let geometry_cases = [
    GeometryCase {
      name: "point",
      geometry_kind: GeometryKind::Point,
      geometry_types: [
        OGRwkbGeometryType::wkbPoint25D,
        OGRwkbGeometryType::wkbPointM,
        OGRwkbGeometryType::wkbPointZM,
      ],
      supports_optimized_output: true,
    },
    GeometryCase {
      name: "line_string",
      geometry_kind: GeometryKind::LineString,
      geometry_types: [
        OGRwkbGeometryType::wkbLineString25D,
        OGRwkbGeometryType::wkbLineStringM,
        OGRwkbGeometryType::wkbLineStringZM,
      ],
      supports_optimized_output: true,
    },
    GeometryCase {
      name: "polygon",
      geometry_kind: GeometryKind::Polygon,
      geometry_types: [
        OGRwkbGeometryType::wkbPolygon25D,
        OGRwkbGeometryType::wkbPolygonM,
        OGRwkbGeometryType::wkbPolygonZM,
      ],
      supports_optimized_output: true,
    },
    GeometryCase {
      name: "multi_point",
      geometry_kind: GeometryKind::MultiPoint,
      geometry_types: [
        OGRwkbGeometryType::wkbMultiPoint25D,
        OGRwkbGeometryType::wkbMultiPointM,
        OGRwkbGeometryType::wkbMultiPointZM,
      ],
      supports_optimized_output: true,
    },
    GeometryCase {
      name: "multi_line_string",
      geometry_kind: GeometryKind::MultiLineString,
      geometry_types: [
        OGRwkbGeometryType::wkbMultiLineString25D,
        OGRwkbGeometryType::wkbMultiLineStringM,
        OGRwkbGeometryType::wkbMultiLineStringZM,
      ],
      supports_optimized_output: true,
    },
    GeometryCase {
      name: "multi_polygon",
      geometry_kind: GeometryKind::MultiPolygon,
      geometry_types: [
        OGRwkbGeometryType::wkbMultiPolygon25D,
        OGRwkbGeometryType::wkbMultiPolygonM,
        OGRwkbGeometryType::wkbMultiPolygonZM,
      ],
      supports_optimized_output: true,
    },
    GeometryCase {
      name: "geometry_collection",
      geometry_kind: GeometryKind::GeometryCollection,
      geometry_types: [
        OGRwkbGeometryType::wkbGeometryCollection25D,
        OGRwkbGeometryType::wkbGeometryCollectionM,
        OGRwkbGeometryType::wkbGeometryCollectionZM,
      ],
      supports_optimized_output: false,
    },
  ];

  for geometry_case in geometry_cases {
    for (dimension_index, dimension_case) in dimension_cases.iter().enumerate() {
      let temp = TempDir::new().unwrap();
      let path = temp.path().join(format!(
        "{}-{}.gpkg",
        geometry_case.name,
        dimension_case.suffix.to_ascii_lowercase()
      ));
      let wkt = dimensional_geometry_wkt(
        geometry_case.name,
        dimension_case.suffix,
        dimension_case.dimension_values,
      );
      let features = [GpkgFeature {
        id: 1,
        name: Some("dimensional"),
        geometry_wkt: &wkt,
      }];
      write_gpkg(
        &path,
        &[GpkgLayer {
          name: "dimensional",
          geometry_type: geometry_case.geometry_types[dimension_index],
          epsg: Some(4326),
          features: &features,
        }],
      );

      let input = open_gpkg_input(&path, None);
      let geometry_metadata = input.source_metadata().unwrap().geometry.unwrap();
      assert_eq!(
        geometry_metadata.geometry_types,
        vec![geometry_case.geometry_kind],
        "{} {} metadata geometry type",
        geometry_case.name,
        dimension_case.suffix
      );
      assert_eq!(geometry_metadata.has_z, dimension_case.has_z);
      assert_eq!(geometry_metadata.has_m, dimension_case.has_m);

      let session = DataFusionSession::new(None, None).unwrap();
      let bytes = runtime().block_on(async {
        let dataframe = input
          .to_dataframe(session.context(), RowRange::default())
          .await
          .unwrap();
        let mut stream = dataframe.execute_stream().await.unwrap();
        let batch = stream.next().await.unwrap().unwrap();
        binary_value(batch.column_by_name("geometry").unwrap().as_ref(), 0)
      });
      let mut sink = CoordinateSink::default();
      let header = visit_wkb_geometry(&bytes, PolygonRingOrder::Preserve, &mut sink).unwrap();
      assert_eq!(header.kind, geometry_case.geometry_kind);
      assert_eq!(header.dimensions, dimension_case.dimensions);
      assert!(!sink.coordinates.is_empty());
      assert_eq!(sink.coordinates[0].x, 1.0);
      assert_eq!(sink.coordinates[0].y, 2.0);
      assert_eq!(sink.coordinates[0].z, dimension_case.has_z.then_some(3.0));
      assert_eq!(sink.coordinates[0].m, dimension_case.has_m.then_some(4.0));

      let output = temp.path().join("optimized.parquet");
      let optimization_result = runtime().block_on(Pipeline::run(SpatialPipelineOptions {
        input: InputOptions {
          location: path.to_string_lossy().into_owned(),
          ..Default::default()
        },
        output: OutputOptions {
          path: output.clone(),
          mode: OutputMode::Optimized,
          overwrite: true,
          ..Default::default()
        },
        ..Default::default()
      }));
      if !geometry_case.supports_optimized_output {
        let error = optimization_result.unwrap_err();
        assert!(
          error
            .to_string()
            .contains("unsupported geometry kind: GeometryCollection")
        );
        continue;
      }
      optimization_result.unwrap();
      let report = validate(&output).unwrap();
      assert!(
        !report.has_errors(),
        "{} {} output failed validation:\n{report}",
        geometry_case.name,
        dimension_case.suffix
      );
    }
  }
}

fn dimensional_geometry_wkt(name: &str, suffix: &str, dimension_values: &str) -> String {
  let first = format!("1 2 {dimension_values}");
  let second = format!("5 2 {dimension_values}");
  let third = format!("5 6 {dimension_values}");
  match name {
    "point" => format!("POINT {suffix} ({first})"),
    "line_string" => format!("LINESTRING {suffix} ({first}, {second})"),
    "polygon" => format!("POLYGON {suffix} (({first}, {second}, {third}, {first}))"),
    "multi_point" => format!("MULTIPOINT {suffix} (({first}), ({second}))"),
    "multi_line_string" => {
      format!("MULTILINESTRING {suffix} (({first}, {second}), ({second}, {third}))")
    }
    "multi_polygon" => {
      format!("MULTIPOLYGON {suffix} ((({first}, {second}, {third}, {first})))")
    }
    "geometry_collection" => {
      format!("GEOMETRYCOLLECTION {suffix} (POINT {suffix} ({first}))")
    }
    _ => unreachable!("unsupported dimensional geometry fixture: {name}"),
  }
}

#[test]
fn gpkg_input_respects_batch_limit() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("points.gpkg");
  let features = [
    GpkgFeature {
      id: 1,
      name: Some("a"),
      geometry_wkt: "POINT (0 0)",
    },
    GpkgFeature {
      id: 2,
      name: Some("b"),
      geometry_wkt: "POINT (1 1)",
    },
    GpkgFeature {
      id: 3,
      name: Some("c"),
      geometry_wkt: "POINT (2 2)",
    },
  ];
  write_gpkg(
    &path,
    &[GpkgLayer {
      name: "points",
      geometry_type: OGRwkbGeometryType::wkbPoint,
      epsg: Some(4326),
      features: &features,
    }],
  );

  let input = open_gpkg_input(&path, None);
  let session = DataFusionSession::new(None, None).unwrap();
  let rows = runtime().block_on(async {
    let dataframe = input
      .to_dataframe(session.context(), RowRange::new(0, Some(2)))
      .await
      .unwrap();
    let mut stream = dataframe.execute_stream().await.unwrap();
    let mut rows = 0usize;
    while let Some(batch) = stream.next().await {
      rows += batch.unwrap().num_rows();
    }
    rows
  });

  assert_eq!(rows, 2);
}
