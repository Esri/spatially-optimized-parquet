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

use crate::geometry::{Extent2D, GeometryKind};
use crate::input::{InputOpenOptions, InputSource, RowRange, SourceFormat, open_input};
use crate::session::DataFusionSession;

fn runtime() -> Runtime {
  Runtime::new().unwrap()
}

struct GpkgFeature<'a> {
  id: i32,
  name: Option<&'a str>,
  geometry_wkt: &'a str,
}

struct GpkgLayerSpec<'a> {
  name: &'a str,
  geometry_type: OGRwkbGeometryType::Type,
  epsg: Option<u32>,
  features: &'a [GpkgFeature<'a>],
}

fn write_gpkg(path: &Path, layers: &[GpkgLayerSpec<'_>]) {
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
    &[GpkgLayerSpec {
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
  assert_eq!(input.total_rows().unwrap(), 2);
  let schema = input.schema().unwrap();
  assert!(schema.field_with_name("id").is_ok());
  assert!(schema.field_with_name("geometry").is_ok());

  let geometry_spec = input.inferred_geometry_spec().unwrap().unwrap();
  assert_eq!(geometry_spec.column, "geometry");
  assert_eq!(geometry_spec.geometry_kind, Some(GeometryKind::Point));

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
    let mut stream = input.read_batches(RowRange::default()).await.unwrap();
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

  assert_eq!(ids, vec![2, 1]);
  assert_eq!(names, vec!["late".to_string(), "early".to_string()]);
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
      GpkgLayerSpec {
        name: "points",
        geometry_type: OGRwkbGeometryType::wkbPoint,
        epsg: Some(4326),
        features: &point_features,
      },
      GpkgLayerSpec {
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
      GpkgLayerSpec {
        name: "points",
        geometry_type: OGRwkbGeometryType::wkbPoint,
        epsg: Some(4326),
        features: &point_features,
      },
      GpkgLayerSpec {
        name: "polygons",
        geometry_type: OGRwkbGeometryType::wkbPolygon,
        epsg: Some(4326),
        features: &polygon_features,
      },
    ],
  );

  let input = open_gpkg_input(&path, Some("polygons"));
  assert_eq!(input.total_rows().unwrap(), 1);
  let geometry_spec = input.inferred_geometry_spec().unwrap().unwrap();
  assert_eq!(geometry_spec.column, "geometry");
  assert_eq!(geometry_spec.geometry_kind, Some(GeometryKind::Polygon));
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
    &[GpkgLayerSpec {
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
    &[GpkgLayerSpec {
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
    &[GpkgLayerSpec {
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
fn open_input_reports_layer_metadata_for_unknown_layer_name() {
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
      GpkgLayerSpec {
        name: "points",
        geometry_type: OGRwkbGeometryType::wkbPoint,
        epsg: Some(4326),
        features: &point_features,
      },
      GpkgLayerSpec {
        name: "polygons",
        geometry_type: OGRwkbGeometryType::wkbPolygon,
        epsg: Some(4326),
        features: &polygon_features,
      },
    ],
  );

  let err = match runtime().block_on(open_input(
    SourceFormat::GeoPackage,
    &InputOpenOptions::new(
      path.to_string_lossy().into_owned(),
      Some("missing".to_string()),
    ),
  )) {
    Ok(_) => panic!("expected unknown GeoPackage layer to fail"),
    Err(err) => err,
  };
  let message = format!("{err:#}");
  assert!(message.contains("was not found"));
  assert!(message.contains("- points (geometry: Point, features: 1)"));
  assert!(message.contains("- polygons (geometry: Polygon, features: 1)"));
}

#[test]
fn open_input_reports_generic_and_non_spatial_layer_hints() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("mixed.gpkg");
  let generic_features = [GpkgFeature {
    id: 1,
    name: Some("generic"),
    geometry_wkt: "POINT (0 0)",
  }];
  write_gpkg(
    &path,
    &[
      GpkgLayerSpec {
        name: "generic_spatial",
        geometry_type: OGRwkbGeometryType::wkbUnknown,
        epsg: Some(4326),
        features: &generic_features,
      },
      GpkgLayerSpec {
        name: "non_spatial",
        geometry_type: OGRwkbGeometryType::wkbNone,
        epsg: None,
        features: &[],
      },
    ],
  );

  let err = match runtime().block_on(open_input(
    SourceFormat::GeoPackage,
    &InputOpenOptions::new(path.to_string_lossy().into_owned(), None),
  )) {
    Ok(_) => panic!("expected mixed GeoPackage without --layer to fail"),
    Err(err) => err,
  };
  let message = format!("{err:#}");
  assert!(message.contains("- generic_spatial (geometry: Point, features: 1)"));
  assert!(message.contains("- non_spatial (geometry: None, features: 0)"));
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
    &[GpkgLayerSpec {
      name: "generic_spatial",
      geometry_type: OGRwkbGeometryType::wkbUnknown,
      epsg: Some(4326),
      features: &generic_features,
    }],
  );

  let input = open_gpkg_input(&path, None);
  let geometry_spec = input.inferred_geometry_spec().unwrap().unwrap();
  assert_eq!(
    geometry_spec.geometry_kind,
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
    &[GpkgLayerSpec {
      name: "points",
      geometry_type: OGRwkbGeometryType::wkbPoint,
      epsg: Some(4326),
      features: &features,
    }],
  );

  let input = open_gpkg_input(&path, None);
  let rows = runtime().block_on(async {
    let mut stream = input.read_batches(RowRange::new(0, Some(2))).await.unwrap();
    let mut rows = 0usize;
    while let Some(batch) = stream.next().await {
      rows += batch.unwrap().num_rows();
    }
    rows
  });

  assert_eq!(rows, 2);
}
