//! Consultas geométricas analíticas para contacto de punta esférica.
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

/// Adaptador experimental del formato STL binario con BVH en XY.
pub mod stl;

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
