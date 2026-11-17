//! CSS Box Sizing 3's intrinsic size keywords against literal Chrome.
use crate::browser_cases::{css_rows, lay_out_with, mismatches, props, Rows};

#[test]
fn intrinsic_size_keywords_match_chrome() {
    let mut wrong = Vec::new();
    for line in include_str!("fixtures/browser_intrinsic.tsv")
        .lines()
        .filter(|l| !l.is_empty())
    {
        let field: Vec<_> = line.split('\t').collect();
        let (name, root) = (field[0], field[1]);
        let mut nodes: Vec<(u32, u32, Rows)> = Vec::new();
        let mut texts = Vec::new();
        let mut images = Vec::new();
        for node in field[2].split(" & ") {
            let part: Vec<_> = node.splitn(4, '>').collect();
            let id: u32 = part[0].parse().unwrap();
            nodes.push((id, part[1].parse().unwrap(), css_rows(part[2])));
            match part[3].strip_prefix("@img") {
                Some(size) => {
                    let (w, h) = size.split_once('x').unwrap();
                    images.push((id, Some((w.parse().unwrap(), h.parse().unwrap()))));
                }
                None if !part[3].is_empty() => texts.push((id, part[3])),
                None => {}
            }
        }
        let want: Vec<(u32, [f32; 4])> = field[3]
            .split(' ')
            .map(|frame| {
                let (id, rect) = frame.split_once('=').unwrap();
                let v: Vec<f32> = rect.split(',').map(|x| x.parse().unwrap()).collect();
                (id.parse().unwrap(), [v[0], v[1], v[2], v[3]])
            })
            .collect();
        let k = lay_out_with(props(&css_rows(root)), nodes, &texts, &images);
        wrong.extend(mismatches(name, &k, &want));
    }
    assert!(
        wrong.is_empty(),
        "{}\n{} mismatches",
        wrong.join("\n"),
        wrong.len()
    );
}
