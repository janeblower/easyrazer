//! The controller drawing's outlines: SVG path data flattened to polygons that egui can fill,
//! including the concave ones, and hit.

use egui::epaint::{Mesh, Vertex};
use egui::{Color32, Pos2, Shape, Stroke, pos2};

/// Steps a curve is cut into.
const STEPS: usize = 12;

/// The subpaths of absolute SVG path data (`M L H V Q C Z`), as polylines.
pub fn parse(d: &str) -> Vec<Vec<Pos2>> {
    let mut tokens = Vec::new();
    let mut num = String::new();
    let flush = |num: &mut String, tokens: &mut Vec<Token>| {
        if !num.is_empty() {
            tokens.push(Token::Num(num.parse().unwrap_or(0.0)));
            num.clear();
        }
    };
    for c in d.chars() {
        match c {
            'M' | 'L' | 'H' | 'V' | 'Q' | 'C' | 'Z' => {
                flush(&mut num, &mut tokens);
                tokens.push(Token::Cmd(c));
            }
            '-' if !num.is_empty() && !num.ends_with('e') => {
                flush(&mut num, &mut tokens);
                num.push(c);
            }
            '0'..='9' | '.' | '-' | 'e' => num.push(c),
            _ => flush(&mut num, &mut tokens),
        }
    }
    flush(&mut num, &mut tokens);
    let mut out: Vec<Vec<Pos2>> = Vec::new();
    let mut at = pos2(0.0, 0.0);
    let mut cmd = 'M';
    let mut i = 0;
    let next = |i: &mut usize| -> Option<f32> {
        match tokens.get(*i) {
            Some(Token::Num(n)) => {
                *i += 1;
                Some(*n)
            }
            _ => None,
        }
    };
    while i < tokens.len() {
        if let Token::Cmd(c) = tokens[i] {
            cmd = c;
            i += 1;
            if c == 'Z' {
                continue;
            }
        }
        match cmd {
            'M' => {
                let (Some(x), Some(y)) = (next(&mut i), next(&mut i)) else { break };
                at = pos2(x, y);
                out.push(vec![at]);
                cmd = 'L';
            }
            'L' => {
                let (Some(x), Some(y)) = (next(&mut i), next(&mut i)) else { break };
                at = pos2(x, y);
                push(&mut out, at);
            }
            'H' => {
                let Some(x) = next(&mut i) else { break };
                at = pos2(x, at.y);
                push(&mut out, at);
            }
            'V' => {
                let Some(y) = next(&mut i) else { break };
                at = pos2(at.x, y);
                push(&mut out, at);
            }
            'Q' => {
                let (Some(cx), Some(cy), Some(x), Some(y)) = (next(&mut i), next(&mut i), next(&mut i), next(&mut i)) else { break };
                let (p0, c, p1) = (at, pos2(cx, cy), pos2(x, y));
                for s in 1..=STEPS {
                    let t = s as f32 / STEPS as f32;
                    let u = 1.0 - t;
                    push(&mut out, pos2(u * u * p0.x + 2.0 * u * t * c.x + t * t * p1.x, u * u * p0.y + 2.0 * u * t * c.y + t * t * p1.y));
                }
                at = p1;
            }
            'C' => {
                let vals: Vec<f32> = (0..6).filter_map(|_| next(&mut i)).collect();
                let [c1x, c1y, c2x, c2y, x, y] = vals[..] else { break };
                let (p0, c1, c2, p1) = (at, pos2(c1x, c1y), pos2(c2x, c2y), pos2(x, y));
                for s in 1..=STEPS {
                    let t = s as f32 / STEPS as f32;
                    let u = 1.0 - t;
                    let b = |a: f32, b: f32, c: f32, d: f32| u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * c + t * t * t * d;
                    push(&mut out, pos2(b(p0.x, c1.x, c2.x, p1.x), b(p0.y, c1.y, c2.y, p1.y)));
                }
                at = p1;
            }
            _ => i += 1,
        }
    }
    out
}

enum Token {
    Cmd(char),
    Num(f32),
}

fn push(out: &mut Vec<Vec<Pos2>>, p: Pos2) {
    match out.last_mut() {
        Some(path) => path.push(p),
        None => out.push(vec![p]),
    }
}

fn cross(o: Pos2, a: Pos2, b: Pos2) -> f32 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

fn area(points: &[Pos2]) -> f32 {
    points.iter().zip(points.iter().cycle().skip(1)).map(|(a, b)| a.x * b.y - b.x * a.y).sum::<f32>() / 2.0
}

fn inside_triangle(p: Pos2, a: Pos2, b: Pos2, c: Pos2) -> bool {
    let (d1, d2, d3) = (cross(a, b, p), cross(b, c, p), cross(c, a, p));
    !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
}

/// Ear clipping: a simple polygon, convex or not, cut into triangles.
pub fn triangulate(points: &[Pos2]) -> Vec<[usize; 3]> {
    let mut idx: Vec<usize> = (0..points.len()).collect();
    if area(points) < 0.0 {
        idx.reverse();
    }
    let mut out = Vec::new();
    let mut guard = 0;
    while idx.len() > 3 && guard < points.len() * points.len() {
        guard += 1;
        let n = idx.len();
        let ear = (0..n).find(|&i| {
            let (a, b, c) = (points[idx[(i + n - 1) % n]], points[idx[i]], points[idx[(i + 1) % n]]);
            cross(a, b, c) > 0.0 && !idx.iter().any(|&j| ![idx[(i + n - 1) % n], idx[i], idx[(i + 1) % n]].contains(&j) && inside_triangle(points[j], a, b, c))
        });
        let Some(i) = ear else { break };
        out.push([idx[(i + n - 1) % n], idx[i], idx[(i + 1) % n]]);
        idx.remove(i);
    }
    if idx.len() == 3 {
        out.push([idx[0], idx[1], idx[2]]);
    }
    out
}

/// A filled outline with a border.
pub fn filled(points: &[Pos2], fill: Color32, stroke: Stroke) -> Vec<Shape> {
    let mut mesh = Mesh::default();
    for p in points {
        mesh.vertices.push(Vertex { pos: *p, uv: Pos2::ZERO, color: fill });
    }
    for [a, b, c] in triangulate(points) {
        mesh.indices.extend([a as u32, b as u32, c as u32]);
    }
    vec![Shape::mesh(mesh), Shape::closed_line(points.to_vec(), stroke)]
}

/// Even-odd point in polygon.
pub fn contains(points: &[Pos2], p: Pos2) -> bool {
    let mut inside = false;
    let n = points.len();
    for i in 0..n {
        let (a, b) = (points[i], points[(i + n - 1) % n]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_and_closes_parse_into_subpaths() {
        let paths = parse("M0 0 H10 V10 H0 Z M20 20 L30 20 L30 30 Z");
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], vec![pos2(0.0, 0.0), pos2(10.0, 0.0), pos2(10.0, 10.0), pos2(0.0, 10.0)]);
        assert!(contains(&paths[0], pos2(5.0, 5.0)));
        assert!(!contains(&paths[0], pos2(15.0, 5.0)));
    }

    #[test]
    fn a_concave_outline_triangulates_whole() {
        // An L: its notch must stay empty.
        let l = [pos2(0.0, 0.0), pos2(10.0, 0.0), pos2(10.0, 4.0), pos2(4.0, 4.0), pos2(4.0, 10.0), pos2(0.0, 10.0)];
        let tris = triangulate(&l);
        assert_eq!(tris.len(), 4);
        let covered: f32 = tris.iter().map(|[a, b, c]| cross(l[*a], l[*b], l[*c]).abs() / 2.0).sum();
        assert!((covered - 64.0).abs() < 1e-3);
    }

    #[test]
    fn curves_end_where_they_point() {
        let p = parse("M0 0 Q5 10 10 0 C10 5 20 5 20 0");
        assert_eq!(*p[0].last().unwrap(), pos2(20.0, 0.0));
    }
}
