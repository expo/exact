# SVG percentage geometry keeps an obsolete numeric motion value

**Status:** Open
**Systems:** kernel/src/motion.rs, kernel/src/svg/scene.rs, native SVG presentation
**Severity:** P2
**Author:** Codex, repository review
**Date:** 2026-11-12

## Failure

At `f6be99aff76bc8bd881b0ec8daaa2c10ace0b9a2`, changing a circle's radius from a numeric length to a percentage leaves its old numeric target in the motion engine. A running transition continues toward that old target and the scene uses it instead of the newly authored percentage. A responsive shape can therefore keep the wrong size after the change.

`svg_targets` deliberately omits percentage geometry (`kernel/src/motion.rs:395–400, 405–425`). `motion_sync_node` sends the eligible values without retiring those omitted properties (lines 917–926). The shared scene resolver prefers any presented radius to the authored radius (`kernel/src/svg/scene.rs:790–793`), and does the same for the other geometry properties at lines 774–786.

The Linux host retains these samples in `Presented::svg` (`host/linux/src/paint_motion.rs:265–273`) and returns the retained presentation on later paints (`host/linux/src/host.rs:738–747`). Ordinary style edits do not remove that entry. The old numeric endpoint can therefore keep overriding the percentage after the transition finishes.

## Reproduction

This valid 100×100 SVG starts a one-second transition from `r=10px` to `r=20px`, advances to 250ms, and changes `r` to `50%`. Exact currently excludes percentages from motion targets, so the new radius must at least take effect when the old transition is retired. Its authored scene resolves to 50; the scene with motion remains 12.5, then 15, then 20.

Save this as `/tmp/exact-review-20261112/svg-percentage-motion/main.rs`:

```rust
use exact_kernel::{Kernel,NodeType,Offer,Op,StyleId,StyleProps,StyleValue};
use exact_kernel::motion::motion_node;
use exact_kernel::svg::scene::{self,Kind};
fn style(id:u32, rows:&[(StyleId,&str)]) -> Op {
 let mut s=StyleProps::default();
 for &(row,value) in rows { s.set_dynamic(row,&StyleValue::Text(value.into())).unwrap(); }
 Op::SetStyle{id,patch:Box::new(s)}
}
fn main(){
 let mut k=Kernel::with_monospace();
 let r=k.apply(0,1,&[
 Op::CreateView{id:1,node_type:NodeType::Svg},
 Op::CreateView{id:2,node_type:NodeType::SvgCircle},
 style(1,&[(StyleId::Width,"100px"),(StyleId::Height,"100px")]),
 style(2,&[(StyleId::Cx,"50px"),(StyleId::Cy,"50px"),(StyleId::R,"10px"),(StyleId::Transition,"r 1s linear")]),
 Op::SetChildren{id:1,children:vec![2]},Op::AttachRoot{id:1}]).unwrap();
 let mut e=Default::default();
 k.motion_sync(&r).apply(&mut e).unwrap();
 let key=k.node(2).unwrap().key; let node=motion_node(key);
 let radius=k.svg_targets(key)[1].0;
 let r=k.apply(0,2,&[style(2,&[(StyleId::R,"20px")])]).unwrap();
 k.motion_sync(&r).apply(&mut e).unwrap();
 e.advance(0.25).unwrap();
 println!("before percent: engine r={:?}, active={}", e.value(node,radius).map(|v|v.x),e.is_active(node,radius));
 let r=k.apply(0,3,&[style(2,&[(StyleId::R,"50%")])]).unwrap();
 let sync=k.motion_sync(&r); println!("retired={:?}",sync.retired);
 sync.apply(&mut e).unwrap();
 k.compute_layout(1,Offer::definite(100.,100.)).unwrap();
 for at in [0.25,0.5,1.0] {
  e.advance(at).unwrap();
  let root=k.node(1).unwrap();
  let actual=scene::resolve(&k,&root,(0.,0.,100.,100.),&|key,p|e.value(motion_node(key),p),false);
  let plain=scene::resolve(&k,&root,(0.,0.,100.,100.),&|_,_|None,false);
  let circle=|s:&scene::Scene| match &s.items[0].kind {Kind::Shape(v)=>v.circle.unwrap().2,_=>panic!()};
  println!("at {at}: presented radius={}, authored radius={}, active={}",circle(&actual),circle(&plain),e.is_active(node,radius));
 }
}
```

Compiled and run against the existing review artifacts, with no Cargo rebuild:

```sh
rustc --edition=2021 /tmp/exact-review-20261112/svg-percentage-motion/main.rs \
  -L dependency=target/debug/deps \
  --extern exact_kernel=target/debug/deps/libexact_kernel-cd5dba534e1d4ea9.rlib \
  -o /tmp/exact-review-20261112/svg-percentage-motion/repro
/tmp/exact-review-20261112/svg-percentage-motion/repro
```

Observed:

```text
before percent: engine r=Some(12.5), active=true
retired=[]
at 0.25: presented radius=12.5, authored radius=50, active=true
at 0.5: presented radius=15, authored radius=50, active=true
at 1: presented radius=20, authored radius=50, active=false
```

The probe runs the actual kernel synchronization, motion engine, and SVG scene resolver. The host retention path was inspected; no native window was launched.

## Direction

Retire a previously numeric SVG geometry property when its current value is ineligible, and remove the host's corresponding presentation override. Cover numeric-to-percentage changes both during motion and after settling, including `r`, `cx`, `cy`, `x`, `y`, `rx`, and `ry` where applicable.

**Deduplication.** Checked open and closed filesystem issues and the all-state GitHub issue snapshot. No matching report was found. The closed `20261111-svg-view-box-transform-origins-ignore.md` concerns the reference box for transform origins and was closed as correct; it does not concern retained geometry targets.
