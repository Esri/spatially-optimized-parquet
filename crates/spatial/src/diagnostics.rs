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

//! Prints DataFusion physical plans for explicit diagnostic builds.

use arrow_array::RecordBatch;
use datafusion::common::Result;
use datafusion::dataframe::DataFrame;
use datafusion::physical_plan::ExecutionPlan;

pub(crate) struct Diagnostics {
  #[cfg(feature = "print-plan")]
  label: String,
}

impl Diagnostics {
  /// Configure one labeled DataFusion diagnostic event.
  pub(crate) fn with(label: &str) -> Self {
    #[cfg(feature = "print-plan")]
    {
      Self {
        label: label.to_string(),
      }
    }

    #[cfg(not(feature = "print-plan"))]
    {
      let _ = label;
      Self {}
    }
  }

  /// Collect one DataFrame while printing its physical plan when diagnostics are enabled.
  pub(crate) async fn collect(self, dataframe: DataFrame) -> Result<Vec<RecordBatch>> {
    #[cfg(feature = "print-plan")]
    {
      use std::sync::Arc;

      use datafusion::execution::TaskContext;
      use datafusion::physical_plan::collect;

      let (state, logical_plan) = dataframe.into_parts();
      let context = Arc::new(TaskContext::from(&state));
      let plan = state.create_physical_plan(&logical_plan).await?;
      self.print_physical_plan(plan.as_ref());
      Ok(collect(plan, context).await?)
    }

    #[cfg(not(feature = "print-plan"))]
    {
      let _ = self;
      Ok(dataframe.collect().await?)
    }
  }

  /// Print one physical plan when the `print-plan` feature is enabled.
  pub(crate) fn print_physical_plan(&self, plan: &dyn ExecutionPlan) {
    #[cfg(feature = "print-plan")]
    {
      use datafusion::physical_plan::displayable;

      eprintln!(
        "\n=== DataFusion physical plan: {} ===\n{}\n",
        self.label,
        displayable(plan).indent(true)
      );
    }

    #[cfg(not(feature = "print-plan"))]
    {
      let _ = (self, plan);
    }
  }
}
