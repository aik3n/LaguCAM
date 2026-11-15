//! Sobrematerial euclideo G22 para fresa plana de eje vertical (+Z).
//!
//! Para un disco de corte XY de radio r y un sobrematerial uniforme a,
//! el apoyo en cada punto p del STL usa el perfil de dilatacion euclidea:
//!   p.z + a                             si dist_xy(p, eje) <= r
//!   p.z + sqrt(a^2 - (dist_xy-r)^2)      si r < dist_xy <= r+a.
//! El maximo se busca analiticamente en vertices y cara, y por biseccion de
//! una derivada monotona en las aristas. No es un chequeo de colisiones de mango.

use super::{ContactError, Vec3, flat_tip_height_triangle};

/// Altura de la punta plana sobre el offset euclideo del triangulo.
/// `None` no equivale a un movimiento seguro.
pub fn flat_tip_height_triangle_allowance(
    triangle: [Vec3; 3],
    x: f64,
    y: f64,
    radius: f64,
    allowance: f64,
) -> Result<Option<f64>, ContactError> {
    if !x.is_finite()
        || !y.is_finite()
        || !radius.is_finite()
        || radius <= 0.0
        || !allowance.is_finite()
        || allowance < 0.0
        || triangle
            .iter()
            .any(|v| !v.x.is_finite() || !v.y.is_finite() || !v.z.is_finite())
    {
        return Err(ContactError::InvalidInput);
    }
    if allowance == 0.0 {
        return flat_tip_height_triangle(triangle, x, y, radius);
    }
    let outer = radius + allowance;
    let outer2 = outer * outer;
    if !outer2.is_finite() {
        return Err(ContactError::ArithmeticRange);
    }

    // Coordenadas XY relativas al eje de herramienta; Z permanece absoluto.
    let mut p = [(0.0, 0.0, 0.0); 3];
    for (i, vertex) in triangle.iter().enumerate() {
        let dx = vertex.x - x;
        let dy = vertex.y - y;
        if !dx.is_finite() || !dy.is_finite() {
            return Err(ContactError::ArithmeticRange);
        }
        p[i] = (dx, dy, vertex.z);
    }
    let mut best: Option<f64> = None;
    let mut offer = |value: f64| -> Result<(), ContactError> {
        if !value.is_finite() {
            return Err(ContactError::ArithmeticRange);
        }
        best = Some(best.map_or(value, |b| b.max(value)));
        Ok(())
    };

    // Retorna cap vertical de la esfera de offset alrededor del disco;
    // None si el punto esta fuera del radio de influencia r+a.
    let cap = |d: f64| -> Option<f64> {
        if d <= radius {
            Some(allowance)
        } else {
            let beyond = d - radius;
            if beyond > allowance {
                None
            } else {
                Some(
                    ((allowance - beyond) * (allowance + beyond))
                        .max(0.0)
                        .sqrt(),
                )
            }
        }
    };

    // 1. Contacto en vertices.
    for &(px, py, pz) in &p {
        let d = px.hypot(py);
        if !d.is_finite() {
            return Err(ContactError::ArithmeticRange);
        }
        if let Some(extra) = cap(d) {
            offer(pz + extra)?;
        }
    }

    // 2. Contacto en aristas. f(t) es concava en el intervalo valido, ya
    // que la funcion de cap radial es concava y no creciente. Su derivada
    // es monotona, de modo que la raiz se acota por biseccion.
    for i in 0..3 {
        let (ax, ay, az) = p[i];
        let (bx, by, bz) = p[(i + 1) % 3];
        let (hx, hy, hz) = (bx - ax, by - ay, bz - az);
        let h2 = hx * hx + hy * hy;
        if !h2.is_finite() || !hz.is_finite() {
            return Err(ContactError::ArithmeticRange);
        }
        if h2 == 0.0 {
            continue; // Arista proyectada como punto: extremos suficientes.
        }
        let t0 = -(ax * hx + ay * hy) / h2;
        let cross = ax * hy - ay * hx;
        let perp2 = cross * cross / h2;
        if !t0.is_finite() || !perp2.is_finite() {
            return Err(ContactError::ArithmeticRange);
        }
        if perp2 > outer2 {
            continue;
        }
        let half = ((outer2 - perp2) / h2).sqrt();
        let lo = (t0 - half).max(0.0);
        let hi = (t0 + half).min(1.0);
        if !lo.is_finite() || !hi.is_finite() {
            return Err(ContactError::ArithmeticRange);
        }
        if lo > hi {
            continue;
        }
        let eval = |t: f64| -> Result<f64, ContactError> {
            let d = (ax + t * hx).hypot(ay + t * hy);
            if !d.is_finite() {
                return Err(ContactError::ArithmeticRange);
            }
            // La interseccion con la circunferencia puede quedar unas ULP
            // fuera por redondeo: el limite de la cap es cero.
            let extra = cap(d).unwrap_or(0.0);
            let value = az + t * hz + extra;
            if value.is_finite() {
                Ok(value)
            } else {
                Err(ContactError::ArithmeticRange)
            }
        };
        let derivative = |t: f64| -> Result<f64, ContactError> {
            let dx = ax + t * hx;
            let dy = ay + t * hy;
            let d = dx.hypot(dy);
            if !d.is_finite() {
                return Err(ContactError::ArithmeticRange);
            }
            if d <= radius {
                return Ok(hz);
            }
            let beyond = d - radius;
            let extra = cap(d).unwrap_or(0.0);
            let dd = (dx * hx + dy * hy) / d;
            if !dd.is_finite() {
                return Err(ContactError::ArithmeticRange);
            }
            if extra == 0.0 {
                // Derivada unilateral infinita en el borde del offset.
                return Ok(if dd > 0.0 {
                    f64::NEG_INFINITY
                } else if dd < 0.0 {
                    f64::INFINITY
                } else {
                    hz
                });
            }
            Ok(hz - (beyond / extra) * dd)
        };
        offer(eval(lo)?)?;
        offer(eval(hi)?)?;
        if lo == hi {
            continue;
        }
        let dlo = derivative(lo)?;
        let dhi = derivative(hi)?;
        if dlo > 0.0 && dhi < 0.0 {
            let (mut low, mut high) = (lo, hi);
            for _ in 0..72 {
                let mid = low + (high - low) * 0.5;
                if mid == low || mid == high {
                    break;
                }
                if derivative(mid)? > 0.0 {
                    low = mid;
                } else {
                    high = mid;
                }
            }
            offer(eval(low + (high - low) * 0.5)?)?;
        }
    }

    // 3. Contacto interior de cara. En un plano z(x,y), el maximo de
    // z + cap(d) esta sobre el gradiente ascendente, a radio
    // r + a*|grad(z)|/hypot(1, |grad(z)|), salvo que lo corte una arista.
    let (ux, uy, uz) = (p[1].0 - p[0].0, p[1].1 - p[0].1, p[1].2 - p[0].2);
    let (vx, vy, vz) = (p[2].0 - p[0].0, p[2].1 - p[0].1, p[2].2 - p[0].2);
    let det = ux * vy - uy * vx;
    if !det.is_finite() {
        return Err(ContactError::ArithmeticRange);
    }
    if det != 0.0 {
        let gx_num = uz * vy - uy * vz;
        let gy_num = ux * vz - uz * vx;
        let gradient_num = gx_num.hypot(gy_num);
        let denom = gradient_num.hypot(det);
        if !denom.is_finite() {
            return Err(ContactError::ArithmeticRange);
        }
        let (px, py, extra) = if gradient_num == 0.0 {
            (0.0, 0.0, allowance)
        } else {
            let distance = radius + allowance * (gradient_num / denom);
            let sign = det.signum();
            (
                distance * gx_num / gradient_num * sign,
                distance * gy_num / gradient_num * sign,
                allowance * (det.abs() / denom),
            )
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
            offer(p[0].2 + (s / det) * uz + (t / det) * vz + extra)?;
        }
    }
    Ok(best)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tri(z: [f64; 3]) -> [Vec3; 3] {
        [
            Vec3 {
                x: 0.0,
                y: 0.0,
                z: z[0],
            },
            Vec3 {
                x: 50.0,
                y: 0.0,
                z: z[1],
            },
            Vec3 {
                x: 0.0,
                y: 50.0,
                z: z[2],
            },
        ]
    }
    #[test]
    fn horizontal_and_ramp_have_distinct_offsets() {
        let horizontal =
            flat_tip_height_triangle_allowance(tri([5.0, 5.0, 5.0]), 8.0, 8.0, 2.0, 3.0).unwrap();
        assert!((horizontal.unwrap() - 8.0).abs() < 1.0e-10);
        let ramp =
            flat_tip_height_triangle_allowance(tri([0.0, 50.0, 0.0]), 8.0, 8.0, 2.0, 1.0).unwrap();
        assert!((ramp.unwrap() - (10.0 + 2.0_f64.sqrt())).abs() < 1.0e-10);
    }
    #[test]
    fn unchanged_when_zero_and_strict_invalid_input() {
        let ramp = tri([0.0, 50.0, 0.0]);
        for (x, y, r) in [(8.0, 8.0, 2.0), (100.0, 100.0, 2.0), (0.0, 3.0, 4.0)] {
            let expected = flat_tip_height_triangle(ramp, x, y, r).unwrap();
            let actual = flat_tip_height_triangle_allowance(ramp, x, y, r, 0.0).unwrap();
            assert_eq!(actual.map(f64::to_bits), expected.map(f64::to_bits));
        }
        assert_eq!(
            flat_tip_height_triangle_allowance(ramp, 8.0, 8.0, 2.0, -1.0),
            Err(ContactError::InvalidInput)
        );
    }
}
