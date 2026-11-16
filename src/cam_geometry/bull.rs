//! G24: contacto exacto de perfil toroidal (bull-nose) con un triangulo STL.
//! La cara inferior es plana hasta K=R-r; la esquina es un cuarto de toro.
//! Para sobrematerial a >= 0, se usa la dilatacion euclidea: esquina r+a,
//! alcance R+a y correccion vertical +a, sin reemplazarla por un offset Z.

use super::{ContactError, Vec3};

fn height_at(
    dx: f64,
    dy: f64,
    z: f64,
    core: f64,
    arc: f64,
    allowance: f64,
) -> Result<f64, ContactError> {
    let d = dx.hypot(dy);
    if !d.is_finite() {
        return Err(ContactError::ArithmeticRange);
    }
    let u = (d - core).max(0.0).min(arc);
    let root = (arc * arc - u * u).max(0.0).sqrt();
    // arc-root, sin cancelacion numerica cuando u es muy pequeno.
    let penalty = u * u / (arc + root);
    let value = z + allowance - penalty;
    if !value.is_finite() {
        return Err(ContactError::ArithmeticRange);
    }
    Ok(value)
}

/// Maximo Z admisible de la punta inferior de una fresa toroidal sobre un
/// triangulo, eje +Z, centro XY fijo, en unidades originales del STL.
///
/// R: radio externo de la fresa; r: radio de esquina, 0 < r <= R.
/// El resultado considera la envolvente de la punta cortante; NO asegura
/// ausencia de colision con el mango, la sujecion ni movimientos entre puntos.
///
/// Los maximos de una funcion concava sobre cada arista se buscan por
/// seccion aurea; el unico estacionario de una cara inclinada es analitico.
/// Se comprueban expresamente extremos y caras verticales/degeneradas.
pub fn bull_tip_height_triangle(
    triangle: [Vec3; 3],
    x: f64,
    y: f64,
    outer_radius: f64,
    corner_radius: f64,
    allowance: f64,
) -> Result<Option<f64>, ContactError> {
    if !x.is_finite()
        || !y.is_finite()
        || !outer_radius.is_finite()
        || outer_radius <= 0.0
        || !corner_radius.is_finite()
        || corner_radius <= 0.0
        || corner_radius > outer_radius
        || !allowance.is_finite()
        || allowance < 0.0
        || triangle.iter().any(|v| !v.finite())
    {
        return Err(ContactError::InvalidInput);
    }
    let reach = outer_radius + allowance;
    let arc = corner_radius + allowance;
    let core = outer_radius - corner_radius;
    let reach2 = reach * reach;
    if !reach2.is_finite() || !arc.is_finite() || !(arc * arc).is_finite() {
        return Err(ContactError::ArithmeticRange);
    }
    let mut p = [(0.0_f64, 0.0_f64, 0.0_f64); 3];
    for (i, v) in triangle.iter().enumerate() {
        let px = v.x - x;
        let py = v.y - y;
        if !px.is_finite() || !py.is_finite() {
            return Err(ContactError::ArithmeticRange);
        }
        p[i] = (px, py, v.z);
    }
    let mut best = None::<f64>;
    let mut offer = |value: f64| {
        best = Some(best.map_or(value, |previous| previous.max(value)));
    };

    // Vertices dentro del disco de alcance.
    for &(px, py, pz) in &p {
        let d2 = px * px + py * py;
        if !d2.is_finite() {
            return Err(ContactError::ArithmeticRange);
        }
        if d2 <= reach2 {
            offer(height_at(px, py, pz, core, arc, allowance)?);
        }
    }

    // Maximo sobre cada arista. Z es afin, y el perfil negativo del toro
    // es concavo y radial no creciente; por tanto no hay maximos ocultos.
    for i in 0..3 {
        let (ax, ay, az) = p[i];
        let (bx, by, bz) = p[(i + 1) % 3];
        let (hx, hy, hz) = (bx - ax, by - ay, bz - az);
        let h2 = hx * hx + hy * hy;
        if !h2.is_finite() {
            return Err(ContactError::ArithmeticRange);
        }
        if h2 == 0.0 {
            continue; // Arista vertical: extremos ya considerados.
        }
        let cross = ax * hy - ay * hx;
        let dot = ax * hx + ay * hy;
        let t0 = -dot / h2;
        let perp2 = cross * cross / h2;
        if !t0.is_finite() || !perp2.is_finite() {
            return Err(ContactError::ArithmeticRange);
        }
        if perp2 > reach2 {
            continue;
        }
        let half = ((reach2 - perp2) / h2).sqrt();
        let lo = (t0 - half).max(0.0);
        let hi = (t0 + half).min(1.0);
        if !lo.is_finite() || !hi.is_finite() {
            return Err(ContactError::ArithmeticRange);
        }
        if lo > hi {
            continue;
        }
        let eval = |t: f64| -> Result<f64, ContactError> {
            height_at(ax + hx * t, ay + hy * t, az + hz * t, core, arc, allowance)
        };
        offer(eval(lo)?);
        offer(eval(hi)?);
        // La busqueda acotada no requiere derivadas ni tolerancias geometricas.
        let (mut left, mut right) = (lo, hi);
        const GOLD: f64 = 0.618_033_988_749_894_9;
        let mut t1 = right - GOLD * (right - left);
        let mut t2 = left + GOLD * (right - left);
        let mut f1 = eval(t1)?;
        let mut f2 = eval(t2)?;
        for _ in 0..96 {
            if f1 < f2 {
                left = t1;
                t1 = t2;
                f1 = f2;
                t2 = left + GOLD * (right - left);
                f2 = eval(t2)?;
            } else {
                right = t2;
                t2 = t1;
                f2 = f1;
                t1 = right - GOLD * (right - left);
                f1 = eval(t1)?;
            }
        }
        offer(f1.max(f2));
    }

    // Cara con proyeccion XY de area no nula. Su Z es un plano afin.
    let (ux, uy, uz) = (p[1].0 - p[0].0, p[1].1 - p[0].1, p[1].2 - p[0].2);
    let (vx, vy, vz) = (p[2].0 - p[0].0, p[2].1 - p[0].1, p[2].2 - p[0].2);
    let det = ux * vy - uy * vx;
    if !det.is_finite() {
        return Err(ContactError::ArithmeticRange);
    }
    if det != 0.0 {
        let gx = (uz * vy - uy * vz) / det;
        let gy = (ux * vz - uz * vx) / det;
        let grad = gx.hypot(gy);
        if !grad.is_finite() {
            return Err(ContactError::ArithmeticRange);
        }
        let (px, py) = if grad == 0.0 {
            (0.0, 0.0)
        } else {
            // Maximo sin restricciones: K+r_eff*m/sqrt(1+m^2) en direccion gradiente.
            let d = core + arc * (grad / grad.hypot(1.0));
            (d * gx / grad, d * gy / grad)
        };
        let qx = px - p[0].0;
        let qy = py - p[0].1;
        let s = qx * vy - qy * vx;
        let t = ux * qy - uy * qx;
        if !s.is_finite() || !t.is_finite() {
            return Err(ContactError::ArithmeticRange);
        }
        let inside = if det > 0.0 {
            s >= 0.0 && t >= 0.0 && s + t <= det
        } else {
            s <= 0.0 && t <= 0.0 && s + t >= det
        };
        if inside {
            let z = p[0].2 + (s / det) * uz + (t / det) * vz;
            offer(height_at(px, py, z, core, arc, allowance)?);
        }
    }
    Ok(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f64, y: f64, z: f64) -> Vec3 {
        Vec3 { x, y, z }
    }

    #[test]
    fn horizontal_face_and_slope_have_known_values() {
        let horizontal = [v(0.0, 0.0, 5.0), v(100.0, 0.0, 5.0), v(0.0, 100.0, 5.0)];
        let ramp = [v(0.0, 0.0, 0.0), v(100.0, 0.0, 100.0), v(0.0, 100.0, 0.0)];
        for a in [0.0, 1.0, 2.0] {
            let h = bull_tip_height_triangle(horizontal, 10.0, 10.0, 6.0, 1.0, a)
                .unwrap()
                .unwrap();
            assert!((h - (5.0 + a)).abs() < 1e-9);
            let z = bull_tip_height_triangle(ramp, 10.0, 10.0, 6.0, 1.0, a)
                .unwrap()
                .unwrap();
            let expected = 10.0 + 5.0 + 1.0 * (2.0_f64.sqrt() - 1.0) + a * 2.0_f64.sqrt();
            assert!((z - expected).abs() < 1e-8, "{z} != {expected}");
        }
    }

    #[test]
    fn sphere_limit_and_tangential_edge_are_handled() {
        let tri = [v(0.0, 0.0, 0.0), v(100.0, 0.0, 100.0), v(0.0, 100.0, 0.0)];
        let z = bull_tip_height_triangle(tri, 8.0, 8.0, 2.0, 2.0, 0.0)
            .unwrap()
            .unwrap();
        assert!((z - (8.0 + 2.0 * (2.0_f64.sqrt() - 1.0))).abs() < 1e-8);
        let wall = [v(6.0, 0.0, 0.0), v(6.0, 0.0, 10.0), v(6.0, 20.0, 10.0)];
        assert_eq!(
            bull_tip_height_triangle(wall, 0.0, 10.0, 6.0, 2.0, 0.0).unwrap(),
            Some(8.0)
        );
        assert_eq!(
            bull_tip_height_triangle(wall, -1.0, 10.0, 6.0, 2.0, 0.0).unwrap(),
            None
        );
    }

    #[test]
    fn invalid_parameters_and_no_contact() {
        let tri = [v(0.0, 0.0, 0.0), v(10.0, 0.0, 0.0), v(0.0, 10.0, 0.0)];
        assert!(matches!(
            bull_tip_height_triangle(tri, 5.0, 5.0, 6.0, 7.0, 0.0),
            Err(ContactError::InvalidInput)
        ));
        assert!(matches!(
            bull_tip_height_triangle(tri, 5.0, 5.0, 6.0, 1.0, -0.1),
            Err(ContactError::InvalidInput)
        ));
        assert_eq!(
            bull_tip_height_triangle(tri, 100.0, 100.0, 6.0, 1.0, 2.0).unwrap(),
            None
        );
    }
}
