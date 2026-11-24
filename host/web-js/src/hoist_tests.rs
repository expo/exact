//! Lifting marked arm bodies (hoist.rs), and the shape the emitter gives a
//! long `if` chain, a navigation stack's `for`, a page two routes share and
//! a virtualized row's text.
use super::*;

/// `«` and `»` stand for the markers.
fn marked(js: &str) -> String {
    js.replace('«', &OPEN.to_string())
        .replace('»', &CLOSE.to_string())
}

fn lifted(js: &str) -> String {
    lift(&marked(js), 1).unwrap()
}

/// The most function bodies that enclose any `needle` in `js`.
pub(crate) fn function_depth(js: &str, needle: &str) -> usize {
    let at: Vec<usize> = js.match_indices(needle).map(|(i, _)| i).collect();
    assert!(!at.is_empty(), "no {needle}");
    let (mut braces, mut deepest, mut next) = (Vec::new(), 0, 0);
    let mut arrow = false;
    let mut l = Lexer::new(js);
    while let Some((c, i)) = l.next_code() {
        while next < at.len() && at[next] < i {
            deepest = deepest.max(braces.iter().filter(|f| **f).count());
            next += 1;
        }
        match c {
            b'{' => braces.push(arrow),
            b'}' => {
                braces.pop();
            }
            _ => {}
        }
        arrow = c == b'>' && i > 0 && js.as_bytes()[i - 1] == b'=';
    }
    deepest
}

#[test]
fn an_else_chain_becomes_siblings() {
    assert_eq!(
        lifted(
            "when(e0,()=>1,(p)=>{«a(p);»},(p)=>{«when(p,()=>2,(p)=>{«b(p);»},\
             (p)=>{«when(p,()=>3,(p)=>{«c(p);»},(p)=>{«d(p);»});»});»});"
        ),
        "const $h0=(p)=>{b(p);};const $h1=(p)=>{c(p);};const $h2=(p)=>{d(p);};\
         const $h3=(p)=>{when(p,()=>3,$h1,$h2);};\
         when(e0,()=>1,(p)=>{a(p);},(p)=>{when(p,()=>2,$h0,$h3);});"
    );
}

/// The mount's own function runs once: an arm in it, no other arm's,
/// costs nothing where it is.
#[test]
fn an_arm_no_arm_encloses_stays() {
    assert_eq!(
        lifted("when(e0,()=>1,(p)=>{«a(p);»});"),
        "when(e0,()=>1,(p)=>{a(p);});"
    );
}

#[test]
fn a_body_takes_what_it_reads_from_between_it_and_the_top() {
    // `i3` and `e5` are bound between the view's top and the arm; `s_0`
    // and `e0` are not, and stay as they are.
    let js = lifted(
        "const e0=h();each(e0,()=>s_0(),(i3,x3)=>i3(),(p,i3,x3)=>{«const e5=h(p);\
         when(e5,()=>i3(),(p)=>{«t(p,i3,e5,e0,s_0);»});»});",
    );
    assert_eq!(
        js,
        "const $h0=(p,e5,i3)=>{t(p,i3,e5,e0,s_0);};const e0=h();\
         each(e0,()=>s_0(),(i3,x3)=>i3(),(p,i3,x3)=>{const e5=h(p);when(e5,()=>i3(),(p)=>$h0(p,e5,i3));});"
    );
}

#[test]
fn a_spread_reads_its_operand() {
    assert_eq!(
        lifted("(p,i0)=>{«const $r0={};when(p,()=>1,(p)=>{«const $a1={...$r0,k:o.$r0};»});»}"),
        "const $h0=(p,$r0)=>{const $a1={...$r0,k:o.$r0};};\
         (p,i0)=>{const $r0={};when(p,()=>1,(p)=>$h0(p,$r0));}"
    );
}

#[test]
fn a_body_passes_its_inner_arms_reads_on() {
    let js = lifted("{const $v1=1;(b2)=>{«const q=b2;f((p)=>{«g((p)=>{«k(p,q,$v1);»});»});»}}");
    assert_eq!(
        js,
        "{const $h0=(p,q)=>{k(p,q,$v1);};const $h1=(p,q)=>{g((p)=>$h0(p,q));};\
         const $v1=1;(b2)=>{const q=b2;f((p)=>$h1(p,q));}}"
    );
}

/// An argument is read when the arm runs; a closure inside the arm reads
/// the name whenever it runs. Where the two could differ, the body stays.
#[test]
fn a_body_whose_reads_an_argument_could_change_stays() {
    for js in [
        "(a)=>{«let n=0;f((p)=>{«g(n);»});»}",
        "(a)=>{«{var n=0;}f((p)=>{«g(n);»});»}",
        "(a)=>{«f((p)=>{«a=1;»});»}",
        "(a)=>{«f((p)=>{«a += 1;»});»}",
        "(a)=>{«f((p)=>{«a++;»});»}",
        "(a)=>{«f((p)=>{«++a;»});»}",
        // Declared further on in its block, so the top's `x` is not it.
        "const x=1;(a)=>{«f((p)=>{«g(x);»});const x=2;»}",
        // Initialized only after the arm is made.
        "(a)=>{«const x=f((p)=>{«g(()=>x);»});»}",
        // Written after the arm is made, outside it.
        "(n)=>{«const get=f((p)=>{«return ()=>n;»});n=2;»}",
        // Names from a pattern, and a pattern written.
        "(q)=>{«{const {a}=o;f((p)=>{«g(a);»});}»}",
        "(a)=>{«f((p)=>{«({a}=o);»});»}",
        // What a `function` binds for itself.
        "(q)=>{«f((p)=>{«g(this,arguments);»});»}",
        // A head that is more than names.
        "(q)=>{«f((p=1)=>{«return p+1;»});»}",
        "(q)=>{«o.map(({a})=>{«g(a);»});»}",
        "(q)=>{«o.map((...a)=>{«g(a);»});»}",
    ] {
        assert_eq!(lifted(js), js.replace(['«', '»'], ""), "{js}");
    }
}

#[test]
fn literals_and_concise_arrows_are_not_code_or_scopes() {
    // Braces, `=>` and names inside literals change nothing, and a concise
    // arrow's parameter is gone after its body.
    let js = lifted(
        "(e1)=>{«f((i3)=>i3(),\"{(i9)=>{\",/[}/]{/g,`${{a:e1}}}`,(p)=>{«g(p,\"}\",`${e1}`,i3,/}/);»});»}",
    );
    assert_eq!(
        js,
        "const $h0=(p,e1)=>{g(p,\"}\",`${e1}`,i3,/}/);};\
         (e1)=>{f((i3)=>i3(),\"{(i9)=>{\",/[}/]{/g,`${{a:e1}}}`,(p)=>$h0(p,e1));}"
    );
}

/// A loop head's names keep the arms that read them in place, and a
/// concise arrow's end at its conditional's `:`.
#[test]
fn loop_heads_and_conditionals_end_their_names() {
    assert_eq!(
        lifted("(q)=>{«for(const x of o)if(c)a();else f((p)=>{«g(x);»});f((p)=>{«h(y);»});»}"),
        "const $h0=(p)=>{h(y);};(q)=>{for(const x of o)if(c)a();else f((p)=>{g(x);});f($h0);}"
    );
    assert_eq!(
        lifted("(q)=>{«f(c?x=>x:(p)=>{«g(x);»},x=>a?x:(p)=>{«h(x);»});»}"),
        "const $h0=(p)=>{g(x);};const $h1=(p,x)=>{h(x);};(q)=>{f(c?x=>x:$h0,x=>a?x:(p)=>$h1(p,x));}"
    );
}

/// What the emitter does not write is not followed: such a view is left
/// as it is.
#[test]
fn a_view_with_what_the_reader_does_not_follow_stays() {
    for js in [
        "(a)=>{«[a]=o;f((p)=>{«g(a);»});»}",
        "(a)=>{«const {b=a}=o;f((p)=>{«g(a,b);»});»}",
        "(q)=>{«f((p)=>{«g(this);»});»}",
        "(q)=>{«const m=function(){};f((p)=>{«g(m);»});»}",
    ] {
        assert_eq!(lifted(js), js.replace(['«', '»'], ""), "{js}");
    }
}

/// An indexed write writes a member, not the name; a property named like
/// a keyword is a property; a key named like a declaration is a key.
#[test]
fn members_and_keys_are_not_names() {
    assert_eq!(
        lifted("(q)=>{«const $a1={};f((p)=>{«$a1[3]=1;»});»}"),
        "const $h0=(p,$a1)=>{$a1[3]=1;};(q)=>{const $a1={};f((p)=>$h0(p,$a1));}"
    );
    assert_eq!(
        lifted("(q)=>{«g(b);p.catch((e)=>{«h(e,q);»});»}"),
        "const $h0=(e,q)=>{h(e,q);};(q)=>{g(b);p.catch((e)=>$h0(e,q));}"
    );
    assert_eq!(
        lifted("(x)=>{«f((p)=>{«return {const:x};»});»}"),
        "const $h0=(p,x)=>{return {const:x};};(x)=>{f((p)=>$h0(p,x));}"
    );
    // A nested pattern's names are the declaration's, in its block.
    assert_eq!(
        lifted("(q)=>{«const b=0;{const {a:{b}}=o;f((p)=>{«g(b);»});}»}"),
        "(q)=>{const b=0;{const {a:{b}}=o;f((p)=>{g(b);});}}"
    );
}

/// A `/` after a postfix `++` divides; after a control statement's head it
/// starts a regex.
#[test]
fn slashes_after_operands_and_heads() {
    assert_eq!(
        lifted("(q)=>{«f((p)=>{«return p++/2+\"}\";»});»}"),
        "const $h0=(p)=>{return p++/2+\"}\";};(q)=>{f($h0);}"
    );
    assert_eq!(
        lifted("(q)=>{«f((p)=>{«if(p)/[{]/.test(q);»});»}"),
        "const $h0=(p,q)=>{if(p)/[{]/.test(q);};(q)=>{f((p)=>$h0(p,q));}"
    );
}

#[test]
fn a_division_after_a_string_is_no_regex() {
    assert_eq!(
        lifted("(q)=>{«f((p)=>{«return \"6\"/2;»},`${q,o}`);»}"),
        "const $h0=(p)=>{return \"6\"/2;};(q)=>{f($h0,`${q,o}`);}"
    );
}

#[test]
fn equal_bodies_are_declared_once() {
    assert_eq!(
        lifted("(q)=>{«when(e0,()=>1,(p)=>{«a(p);»},(p)=>{«a(p);»});»}"),
        "const $h0=(p)=>{a(p);};(q)=>{when(e0,()=>1,$h0,$h0);}"
    );
}

#[test]
fn a_short_body_stays_in_place() {
    let js = marked("(q)=>{«when(e0,()=>1,(p)=>{«a(p);»});»}");
    assert_eq!(
        lift(&js, 64).unwrap(),
        "(q)=>{when(e0,()=>1,(p)=>{a(p);});}"
    );
}

fn emitted(source: &str) -> String {
    let plan = contract::compile(source).unwrap();
    crate::emit::emit(&plan, false, false).unwrap().js
}

/// A page of `rows` bound rows, each different, as an arm's body.
fn page(indent: &str, name: &str, rows: usize) -> String {
    (0..rows)
        .map(|j| {
            format!(
                "{indent}text `{name} row {j}: ${{n + {j}}}` padding={}\n",
                j % 7
            )
        })
        .collect()
}

/// No arm of a long `if` chain is inside another's function: each sits in
/// the mount's function or one lifted function within it.
#[test]
fn a_long_if_chain_emits_its_arms_side_by_side() {
    let mut source =
        String::from("component App\n  state r = 0\n  state n = 0\n  view\n    column\n");
    for k in 0..12 {
        source.push_str(&match k {
            0 => "      if r == 0\n".to_string(),
            11 => "      else\n".to_string(),
            k => format!("      else if r == {k}\n"),
        });
        // Each arm owns state, which it writes by index (`$a1[3]=…`).
        source.push_str(&format!("        Counter(n={k})\n"));
        source.push_str(&page("        ", &format!("arm {k}"), 250));
    }
    source.push_str(
        "\ncomponent Counter\n  props\n    n: number\n  state c = n\n  action bump\n    c = c + 1\n  \
         view\n    button `${c}` press=bump\n",
    );
    for site_attrs in [false, true] {
        let plan = contract::compile(&source).unwrap();
        let js = crate::emit::emit(&plan, site_attrs, site_attrs).unwrap().js;
        for k in 0..12 {
            let depth = function_depth(&js, &format!("arm {k} row 0"));
            assert!(depth <= 2, "arm {k} is {depth} functions deep: {js}");
        }
    }
}

/// A navigation stack's `for` holds its pages by reference: no page is
/// inside the row's function or another page's.
#[test]
fn a_stack_for_holds_its_pages_by_reference() {
    let mut source = String::from("routes nav\n  home \"/\"\n");
    for k in 0..6 {
        source.push_str(&format!("    p{k} \"/p/{k}\"\n"));
    }
    source.push_str(
        "\ncomponent App\n  state n = 0\n  view\n    main navigationKey=`${top(nav).id}`\n      \
         for e in stack(nav) key=e.id\n        column navigationKey=`${e.id}`\n",
    );
    for k in 0..6 {
        source.push_str(&format!("          if e.name == \"p{k}\"\n"));
        source.push_str(&page("            ", &format!("page {k}"), 250));
    }
    let js = emitted(&source);
    for k in 0..6 {
        let depth = function_depth(&js, &format!("page {k} row 0"));
        assert!(depth <= 2, "page {k} is {depth} functions deep: {js}");
    }
}

/// A page two routes share is one shared mount: it is beside the row too.
#[test]
fn a_shared_page_is_not_inside_the_row() {
    let mut source = String::from("routes nav\n  home \"/\"\n    a \"/a\"\n    b \"/b\"\n\n");
    source.push_str(
        "component App\n  state n = 0\n  view\n    main navigationKey=`${top(nav).id}`\n      \
         for e in stack(nav) key=e.id\n        column navigationKey=`${e.id}`\n          \
         if e.name == \"a\"\n            Page(n=n)\n          if e.name == \"b\"\n            Page(n=n)\n\n\
         component Page\n  props\n    n: number\n  view\n    column\n",
    );
    // Rows of different shapes, so the page is one mount the arms share.
    let props = [
        "padding",
        "margin",
        "opacity",
        "gap",
        "width",
        "height",
        "flex-grow",
    ];
    for j in 0..150 {
        let prop = props[j % props.len()];
        source.push_str(&format!(
            "      text `shared row {j}: ${{n}}` {prop}=(n > {j} ? 1 : 0)\n"
        ));
    }
    let js = emitted(&source);
    let depth = function_depth(&js, "shared row 0");
    assert!(
        depth <= 2,
        "the shared page is {depth} functions deep: {js}"
    );
}

/// A virtualized list's row text reads its regions as functions too: a
/// long chain there is side by side.
#[test]
fn a_virtualized_rows_text_reads_its_arms_side_by_side() {
    let mut source = String::from(
        "component App\n  state n = 0\n  state xs = [0, 1, 2]\n  view\n    list virtualized=true height=400\n      for x in xs key=x\n        column\n",
    );
    for k in 0..8 {
        source.push_str(&match k {
            0 => "          if x == 0\n".to_string(),
            7 => "          else\n".to_string(),
            k => format!("          else if x == {k}\n"),
        });
        source.push_str(&page("            ", &format!("arm {k}"), 250));
    }
    let js = emitted(&source);
    for k in 0..8 {
        let depth = function_depth(&js, &format!("arm {k} row 0"));
        assert!(depth <= 3, "arm {k} is {depth} functions deep: {js}");
    }
}
