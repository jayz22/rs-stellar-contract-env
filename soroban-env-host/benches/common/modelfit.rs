use std::collections::HashSet;
use std::str::FromStr;

use linregress::{FormulaRegressionBuilder, RegressionDataBuilder};
use na::U1;
use num_traits::Pow;

use nalgebra::{self as na, OMatrix, OVector};

#[derive(Debug, Default, Clone, PartialEq, PartialOrd)]
pub struct FPCostModel {
    pub const_param: f64,
    pub lin_param: f64,
    pub r_squared: f64,
}

// We have to use a floating-point cost model in order to interface with the
// numerical optimizer below -- using integral types causes it to get very
// confused due to rounding/truncation.
impl FPCostModel {
    pub fn new(params: &[f64], r2: f64) -> Self {
        let mut fcm = FPCostModel::default();
        fcm.const_param = params[0];
        fcm.lin_param = params[1];
        fcm.r_squared = r2;
        fcm
    }
    // This is the same as the 'evaluate' function in the integral cost model,
    // just using f64 ops rather than saturating integer ops.
    pub fn evaluate(&self, input: f64) -> f64 {
        let mut res = self.const_param;
        if input.is_finite() && input != 0.0 {
            res += self.lin_param * input;
        }
        res
    }
    // Extract the parameters from FPs to integers
    pub fn params_as_u64(&self) -> (u64, u64) {
        let extract_param = |f: f64| -> u64 {
            // clamp the float to 1 digit (to filter noise) then take the ceil
            let f = f64::from_str(format!("{:.1}", f).as_str()).unwrap();
            f.ceil() as u64
        };
        (
            extract_param(self.const_param),
            extract_param(self.lin_param),
        )
    }
}

fn fit_linear_regression(x: Vec<f64>, y: Vec<f64>) -> FPCostModel {
    assert_eq!(x.len(), y.len());
    let data = vec![("Y", y), ("X", x)];
    let data = RegressionDataBuilder::new().build_from(data).unwrap();
    let model = FormulaRegressionBuilder::new()
        .data(&data)
        .formula("Y ~ X")
        .fit()
        .unwrap();
    let r2 = model.rsquared();
    FPCostModel::new(model.parameters(), r2)
}

fn compute_rsquared(x: Vec<f64>, y: Vec<f64>, const_param: f64, lin_param: f64) -> f64 {
    assert_eq!(x.len(), y.len());
    let pred_y: Vec<f64> = x.iter().map(|x| const_param + lin_param * x).collect();
    let y_mean = y.iter().sum::<f64>() / y.len() as f64;
    let ss_res = y.iter().zip(pred_y.iter()).map(|(y, y_pred)| (y - y_pred).pow(2i32)).sum::<f64>();
    let ss_tot = y.iter().map(|y| (y - y_mean).pow(2)).sum::<f64>();
    1f64 - ss_res / ss_tot
}

pub fn fit_model(x: Vec<u64>, y: Vec<u64>) -> FPCostModel {
    assert_eq!(x.len(), y.len());
    let const_model = x.iter().collect::<HashSet<_>>().len() == 1;
    if const_model {
        let const_param = y.iter().sum::<u64>() as f64 / y.len() as f64;
        return FPCostModel {
            const_param,
            lin_param: 0.0,
            r_squared: 0.0, // we are always predicting the mean
        };
    }

    let nrows = x.len();
    let x = x.iter().map(|i| *i as f64).collect::<Vec<_>>();
    let y = y.iter().map(|i| *i as f64).collect::<Vec<_>>();

    // This is the solution that does not pin the x axis. Equivalent to previous solution (with wild intercepts)
    // let mut a = x.clone();
    // a.append(&mut vec![1.0; nrows]);
    // // println!("{}, {}", a.len(), y.len());
    // let a = OMatrix::<f64, na::Dyn, U2>::from_column_slice(&a);
    // let b = OVector::<f64, na::Dyn>::from_row_slice(&y);
    // // println!("{}, {}", a.len(), b.len());
    // let res = lstsq::lstsq(&a, &b, 1e-14).unwrap();
    // assert_eq!(res.solution.len(), 2);
    // let const_param = res.solution[1];
    // let lin_param = res.solution[0];
    // let r_squared = compute_rsquared(x.clone(), y.clone(), const_param, lin_param);
    // FPCostModel{ const_param, lin_param, r_squared }

    // This is the solution that pins x axis to (x0, y0)
    // x0 is not necessary at x=0, it is the first input point
    // here we are just making sure the line produced will always pass through (x0, y0)
    // assume X is mono-increasing
    let x0 = x[0];
    let y0 = y[0];
    println!("{}, {}", x0, y0);
    let a: Vec<f64> = x.iter().map(|x| x - x0).collect();
    let a = OMatrix::<f64, na::Dyn, U1>::from_column_slice(&a);
    let b: Vec<f64> = y.iter().map(|y| y - y0).collect();
    let b = OVector::<f64, na::Dyn>::from_row_slice(&b);
    // println!("{}, {}", a.len(), b.len());
    let lsq_res = lstsq::lstsq(&a, &b, 1e-14).unwrap();
    assert_eq!(lsq_res.solution.len(), 1);
    let lin_param = lsq_res.solution[0];    
    let const_param = y0 - lin_param * x0;
    let r_squared = compute_rsquared(x.clone(), y.clone(), const_param, lin_param);
    let mut res = FPCostModel{ const_param, lin_param, r_squared };

    // second pass: we make sure that the intercept y (x=0) >= 0
    assert!(res.lin_param > 0.0, "negative slope detected, examine your data, or choose a constant model");
    if res.const_param < 0.0 {
        println!("negative intercept detected, will constrain it to 0 and rerun");
        let a = OMatrix::<f64, na::Dyn, U1>::from_column_slice(&x);
        let b = OVector::<f64, na::Dyn>::from_row_slice(&y);
        let lsq_res = lstsq::lstsq(&a, &b, 1e-14).unwrap();
        assert_eq!(lsq_res.solution.len(), 1);
        let lin_param = lsq_res.solution[0];    
        let r_squared = compute_rsquared(x.clone(), y.clone(), 0.0, lin_param);
        res = FPCostModel{ const_param: 0.0, lin_param, r_squared };
    }
    res
}
