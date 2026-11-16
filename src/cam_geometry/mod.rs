//! Consultas geométricas analíticas para contacto de fresas esfericas y planas.
//! Coordenadas `f64`; sin dependencias de STL, interfaz ni controlador CNC.
//! El resultado no verifica colisiones del mango ni del portaherramientas.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    fn finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContactError {
    InvalidInput,
    ArithmeticRange,
}

/// Altura de la punta inferior. None indica que ningún punto del triángulo
/// cae en el cilindro vertical de radio `radius` alrededor de `(x, y)`.
/// Calcula la envolvente inferior esférica, no la colisión de la fresa completa.
pub fn ball_tip_height_triangle(
    triangle: [Vec3; 3],
    x: f64,
    y: f64,
    radius: f64,
) -> Result<Option<f64>, ContactError> {
    if !x.is_finite()
        || !y.is_finite()
        || !radius.is_finite()
        || radius <= 0.0
        || triangle.iter().any(|v| !v.finite())
    {
        return Err(ContactError::InvalidInput);
    }

    // Trasladamos XY a la consulta; reduce la cancelación para modelos alejados del origen.
    let mut p = [(0.0, 0.0, 0.0); 3];
    for (i, v) in triangle.iter().enumerate() {
        p[i] = (v.x - x, v.y - y, v.z);
        if !p[i].0.is_finite() || !p[i].1.is_finite() {
            return Err(ContactError::ArithmeticRange);
        }
    }
    let r2 = radius * radius;
    if !r2.is_finite() {
        return Err(ContactError::ArithmeticRange);
    }
    let mut best: Option<f64> = None;
    let mut offer = |height: f64| -> Result<(), ContactError> {
        if !height.is_finite() {
            return Err(ContactError::ArithmeticRange);
        }
        best = Some(best.map_or(height, |b| b.max(height)));
        Ok(())
    };

    // 1. Contacto en vértices.
    for &(px, py, pz) in &p {
        let d2 = px * px + py * py;
        if !d2.is_finite() {
            return Err(ContactError::ArithmeticRange);
        }
        if d2 <= r2 {
            offer(pz + (r2 - d2).sqrt() - radius)?;
        }
    }

    // 2. Contacto interior en cada arista: máximo exacto de una función cóncava.
    for i in 0..3 {
        let (ax, ay, az) = p[i];
        let (bx, by, bz) = p[(i + 1) % 3];
        let (hx, hy, hz) = (bx - ax, by - ay, bz - az);
        let h2 = hx * hx + hy * hy;
        if !h2.is_finite() {
            return Err(ContactError::ArithmeticRange);
        }
        if h2 == 0.0 {
            continue;
        } // Arista vertical: basta su vértice superior.
        let t0 = -(ax * hx + ay * hy) / h2;
        let cross = ax * hy - ay * hx;
        let perp2 = cross * cross / h2;
        if !t0.is_finite() || !perp2.is_finite() {
            return Err(ContactError::ArithmeticRange);
        }
        if perp2 > r2 {
            continue;
        }
        // Intervalo de la arista contenido en el disco de influencia.
        // Es importante incluir sus extremos: el máximo puede estar exactamente
        // en la circunferencia, aunque ambos vértices estén fuera.
        let half_width = ((r2 - perp2) / h2).sqrt();
        let lo = (t0 - half_width).max(0.0);
        let hi = (t0 + half_width).min(1.0);
        if !lo.is_finite() || !hi.is_finite() {
            return Err(ContactError::ArithmeticRange);
        }
        if lo > hi {
            continue;
        }
        let line_len = h2.sqrt();
        let denom = line_len * line_len.hypot(hz);
        if !denom.is_finite() || denom == 0.0 {
            return Err(ContactError::ArithmeticRange);
        }
        let stationary = t0 + hz * (r2 - perp2).sqrt() / denom;
        if !stationary.is_finite() {
            return Err(ContactError::ArithmeticRange);
        }
        let t = stationary.clamp(lo, hi);
        let dx = ax + t * hx;
        let dy = ay + t * hy;
        let d2 = dx * dx + dy * dy;
        if !d2.is_finite() {
            return Err(ContactError::ArithmeticRange);
        }
        // En un extremo definido por la circunferencia, el redondeo puede
        // producir d2 ligeramente > r2; la raíz matemática en ese punto es 0.
        let root = (r2 - d2).max(0.0).sqrt();
        offer(az + t * hz + root - radius)?;
    }

    // 3. Contacto interior de cara: normal unitaria orientada a +Z.
    let (ux, uy, uz) = (p[1].0 - p[0].0, p[1].1 - p[0].1, p[1].2 - p[0].2);
    let (vx, vy, vz) = (p[2].0 - p[0].0, p[2].1 - p[0].1, p[2].2 - p[0].2);
    let (mut nx, mut ny, mut nz) = (uy * vz - uz * vy, uz * vx - ux * vz, ux * vy - uy * vx);
    let len = nx.hypot(ny).hypot(nz);
    if !len.is_finite() {
        return Err(ContactError::ArithmeticRange);
    }
    if len > 0.0 && nz != 0.0 {
        nx /= len;
        ny /= len;
        nz /= len;
        if nz < 0.0 {
            nx = -nx;
            ny = -ny;
            nz = -nz;
        }
        let px = -radius * nx;
        let py = -radius * ny;
        // Test de pertenencia en proyección XY, sin tolerancia topológica arbitraria.
        let det = ux * vy - uy * vx;
        let qx = px - p[0].0;
        let qy = py - p[0].1;
        let s = qx * vy - qy * vx;
        let t = ux * qy - uy * qx;
        let inside = if det > 0.0 {
            s >= 0.0 && t >= 0.0 && s + t <= det
        } else {
            s <= 0.0 && t <= 0.0 && s + t >= det
        };
        if inside {
            // Barycentría de la cara; evita restar dos alturas grandes.
            let b = s / det;
            let c = t / det;
            let z = p[0].2 + b * uz + c * vz;
            offer(z + radius * nz - radius)?;
        }
    }
    Ok(best)
}

/// Máximo de contacto de una superficie de triángulos; implementación O(N) de referencia.
pub fn ball_tip_height_mesh(
    triangles: &[[Vec3; 3]],
    x: f64,
    y: f64,
    radius: f64,
) -> Result<Option<f64>, ContactError> {
    let mut best: Option<f64> = None;
    for &tri in triangles {
        if let Some(z) = ball_tip_height_triangle(tri, x, y, radius)? {
            best = Some(best.map_or(z, |b| b.max(z)));
        }
    }
    Ok(best)
}

/// Cota inferior del extremo plano de una fresa vertical sobre un triangulo.
///
/// La punta plana ocupa un disco XY cerrado. El contacto es el maximo Z de
/// los puntos del triangulo cuya proyeccion pertenece a ese disco. Se
/// comprueban vertices, intersecciones arista/circunferencia y el maximo
/// interior de cara sobre la circunferencia. Las caras verticales (proyeccion
/// degenerada) quedan cubiertas por las aristas tridimensionales.
/// Ningun resultado certifica ausencia de colisiones laterales o del mango.
pub fn flat_tip_height_triangle(
    triangle: [Vec3; 3],
    x: f64,
    y: f64,
    radius: f64,
) -> Result<Option<f64>, ContactError> {
    if !x.is_finite()
        || !y.is_finite()
        || !radius.is_finite()
        || radius <= 0.0
        || triangle.iter().any(|v| !v.finite())
    {
        return Err(ContactError::InvalidInput);
    }
    let r2 = radius * radius;
    if !r2.is_finite() {
        return Err(ContactError::ArithmeticRange);
    }
    // Trasladar al centro de la herramienta reduce cancelacion numerica.
    let mut p = [(0.0_f64, 0.0_f64, 0.0_f64); 3];
    for (i, vertex) in triangle.iter().enumerate() {
        let dx = vertex.x - x;
        let dy = vertex.y - y;
        if !dx.is_finite() || !dy.is_finite() {
            return Err(ContactError::ArithmeticRange);
        }
        p[i] = (dx, dy, vertex.z);
    }
    let mut best: Option<f64> = None;
    let mut offer = |z: f64| -> Result<(), ContactError> {
        if !z.is_finite() {
            return Err(ContactError::ArithmeticRange);
        }
        best = Some(best.map_or(z, |current| current.max(z)));
        Ok(())
    };

    // 1. Vertices contenidos en el disco, incluidos los de proyeccion igual.
    for &(px, py, pz) in &p {
        let d2 = px * px + py * py;
        if !d2.is_finite() {
            return Err(ContactError::ArithmeticRange);
        }
        if d2 <= r2 {
            offer(pz)?;
        }
    }

    // 2. Para cada arista 3D, su Z es afín en el parámetro t. Dentro
    // del disco el maximo se encuentra en un extremo del intervalo valido.
    for i in 0..3 {
        let (ax, ay, az) = p[i];
        let (bx, by, bz) = p[(i + 1) % 3];
        let (hx, hy, hz) = (bx - ax, by - ay, bz - az);
        let h2 = hx * hx + hy * hy;
        if !h2.is_finite() {
            return Err(ContactError::ArithmeticRange);
        }
        if h2 == 0.0 {
            // Proyeccion de arista vertical: los vertices ya cubren max Z.
            continue;
        }
        let dot = ax * hx + ay * hy;
        let cross = ax * hy - ay * hx;
        let perp2 = cross * cross / h2;
        let t0 = -dot / h2;
        if !dot.is_finite() || !perp2.is_finite() || !t0.is_finite() {
            return Err(ContactError::ArithmeticRange);
        }
        if perp2 > r2 {
            continue;
        }
        let half = ((r2 - perp2) / h2).sqrt();
        let lo = t0 - half;
        let hi = t0 + half;
        if !lo.is_finite() || !hi.is_finite() {
            return Err(ContactError::ArithmeticRange);
        }
        if lo > 1.0 || hi < 0.0 {
            continue;
        }
        offer(az + lo.clamp(0.0, 1.0) * hz)?;
        offer(az + hi.clamp(0.0, 1.0) * hz)?;
    }

    // 3. Si la proyeccion es un triangulo no degenerado, Z es un plano
    // afín. El maximo sobre disco ∩ triangulo tambien puede estar en
    // la circunferencia interior a la cara (direccion del gradiente XY).
    let (ux, uy, uz) = (p[1].0 - p[0].0, p[1].1 - p[0].1, p[1].2 - p[0].2);
    let (vx, vy, vz) = (p[2].0 - p[0].0, p[2].1 - p[0].1, p[2].2 - p[0].2);
    let det = ux * vy - uy * vx;
    if !det.is_finite() {
        return Err(ContactError::ArithmeticRange);
    }
    if det != 0.0 {
        let gx_num = uz * vy - uy * vz;
        let gy_num = ux * vz - uz * vx;
        let grad_len = gx_num.hypot(gy_num);
        if !grad_len.is_finite() {
            return Err(ContactError::ArithmeticRange);
        }
        let (px, py) = if grad_len == 0.0 {
            // Cara horizontal: disco puede quedar completamente dentro
            // del triangulo y ningun vertice/arista tocarlo.
            (0.0, 0.0)
        } else {
            let sign = det.signum();
            (
                radius * (gx_num / grad_len) * sign,
                radius * (gy_num / grad_len) * sign,
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
            offer(p[0].2 + (s / det) * uz + (t / det) * vz)?;
        }
    }
    Ok(best)
}

/// Referencia exhaustiva O(N) para contrastar el indice BVH de fresas planas.
pub fn flat_tip_height_mesh(
    triangles: &[[Vec3; 3]],
    x: f64,
    y: f64,
    radius: f64,
) -> Result<Option<f64>, ContactError> {
    let mut best: Option<f64> = None;
    for &triangle in triangles {
        if let Some(z) = flat_tip_height_triangle(triangle, x, y, radius)? {
            best = Some(best.map_or(z, |current| current.max(z)));
        }
    }
    Ok(best)
}

/// Adaptador experimental del formato STL binario con BVH en XY.
pub mod stl;

pub mod bull;
pub mod contact_solver;
pub mod flat_allowance;
/// Fachada del motor de contacto para futuras operaciones CAM.
pub mod service;

#[cfg(test)]
mod core_tests {
    use super::*;

    #[test]
    fn horizontal_triangle_supports_ball_at_face() {
        let tri = [
            Vec3 {
                x: 0.0,
                y: 0.0,
                z: 5.0,
            },
            Vec3 {
                x: 10.0,
                y: 0.0,
                z: 5.0,
            },
            Vec3 {
                x: 0.0,
                y: 10.0,
                z: 5.0,
            },
        ];
        assert_eq!(ball_tip_height_triangle(tri, 2.0, 2.0, 1.0), Ok(Some(5.0)));
    }

    #[test]
    fn no_contact_returns_none() {
        let tri = [
            Vec3 {
                x: 0.0,
                y: 0.0,
                z: 5.0,
            },
            Vec3 {
                x: 1.0,
                y: 0.0,
                z: 5.0,
            },
            Vec3 {
                x: 0.0,
                y: 1.0,
                z: 5.0,
            },
        ];
        assert_eq!(ball_tip_height_triangle(tri, 100.0, 100.0, 1.0), Ok(None));
    }

    #[test]
    fn flat_disk_supports_vertical_edges_and_horizontal_faces() {
        let horizontal = [
            Vec3 {
                x: 0.0,
                y: 0.0,
                z: 5.0,
            },
            Vec3 {
                x: 40.0,
                y: 0.0,
                z: 5.0,
            },
            Vec3 {
                x: 0.0,
                y: 40.0,
                z: 5.0,
            },
        ];
        assert_eq!(
            flat_tip_height_triangle(horizontal, 8.0, 8.0, 6.0),
            Ok(Some(5.0))
        );
        let vertical = [
            Vec3 {
                x: 5.0,
                y: 0.0,
                z: 0.0,
            },
            Vec3 {
                x: 5.0,
                y: 0.0,
                z: 10.0,
            },
            Vec3 {
                x: 5.0,
                y: 10.0,
                z: 4.0,
            },
        ];
        assert_eq!(
            flat_tip_height_triangle(vertical, 3.0, 5.0, 2.0),
            Ok(Some(7.0))
        );
        assert_eq!(flat_tip_height_triangle(vertical, 0.0, 5.0, 2.0), Ok(None));
    }

    #[test]
    fn flat_disk_maximum_on_interior_circle_is_exact_for_slopes() {
        let ramp = [
            Vec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            Vec3 {
                x: 40.0,
                y: 0.0,
                z: 40.0,
            },
            Vec3 {
                x: 0.0,
                y: 40.0,
                z: 0.0,
            },
        ];
        assert_eq!(flat_tip_height_triangle(ramp, 8.0, 8.0, 1.0), Ok(Some(9.0)));
        assert_eq!(
            flat_tip_height_triangle(ramp, 8.0, 8.0, 6.0),
            Ok(Some(14.0))
        );
        assert_eq!(flat_tip_height_triangle(ramp, 100.0, 100.0, 1.0), Ok(None));
        assert_eq!(
            flat_tip_height_triangle(ramp, 8.0, 8.0, -1.0),
            Err(ContactError::InvalidInput)
        );
    }

    #[test]
    fn invalid_input_is_rejected() {
        let tri = [Vec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        }; 3];
        assert_eq!(
            ball_tip_height_triangle(tri, 0.0, 0.0, 0.0),
            Err(ContactError::InvalidInput)
        );
        assert_eq!(
            ball_tip_height_triangle(tri, f64::NAN, 0.0, 1.0),
            Err(ContactError::InvalidInput)
        );
    }
}
