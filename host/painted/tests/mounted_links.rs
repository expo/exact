use exact_painted::app::{boot_presenter, launch_location, Config};
use serde_json::json;

fn delivery(platform: &str) {
    let source = "routes nav\n  home \"/\"\n    post \"/post/:id\"\n  notfound\ncomponent App\n  action follow(location: string)\n    nav = open(nav, location)\n  view\n    main navigationBack=\"back\" navigationKey=`${top(nav).id}` navigate=follow\n      text top(nav).url\n";
    let plan = contract::compile(source).unwrap().encode();
    {
        let compat = json!({"inputs": {"platform": platform, "store": {"L": "0"}, "webMount": {"origin": "https://site.invalid", "base": "/app/"}}}).to_string();
        let launch = launch_location(
            ["https://site.invalid/app/post/42?q=1#part".into()],
            &compat,
        );
        assert_eq!(launch, "/post/42?q=1#part", "{platform}: cold launch");
        let mut config = Config {
            output_rotation: Default::default(),
            #[cfg(target_os = "macos")]
            painted_boot: None,
            services: Default::default(),
            content_region: None,
            launch,
            plan: plan.clone().into(),
            fallback_plan: None,
            assets: std::path::PathBuf::new(),
            scale: 1.,
            size: (390., 844.),
            agent: true,
            smoke: false,
            shot: None,
            dev_plan: None,
            dev_url: None,
            dev_said: None,
            dev_identity: None,
            card: String::new(),
            vnc: None,
            compat,
            explicit: false,
            entry: None,
            selected_assets: None,
            selected_dir: None,
            module: Ok(None),
            updates: None,
            hatches: None,
            packages: Vec::new(),
            clock_started: None,
            preferences: None,
            page: None,
        };
        let (mut p, error) = boot_presenter::<()>(&mut config, (390., 844.)).unwrap();
        assert!(error.is_none(), "{error:?}");
        let state = exact_runner::agent::state(p.host().runner());
        assert!(state.contains("/post/42?q=1"), "{platform}: cold: {state}");
        assert!(state.contains("\"fragment\":\"part\""), "{state}");
        for (url, expected, fragment) in [
            (
                "https://site.invalid/app/post/42?q=1#part",
                "/post/42?q=1",
                "part",
            ),
            (
                "https://site.invalid/app/post/7?q=2#next",
                "/post/7?q=2",
                "next",
            ),
        ] {
            assert_eq!(p.open_location(url, 0.), None, "{platform}: {url}");
            let state = exact_runner::agent::state(p.host().runner());
            assert!(
                state.contains(&format!("\"fragment\":\"{fragment}\"")),
                "{state}"
            );
            assert!(
                state.contains(expected),
                "{platform}: {}",
                exact_runner::agent::state(p.host().runner())
            );
        }
        assert!(
            p.reload(&plan, ()).unwrap() == exact_painted::presenter::ReloadResult::Applied(None)
        );
        let url = "https://site.invalid/app/post/9?q=3#reload";
        assert_eq!(p.open_location(url, 0.), None, "{platform}: after reload");
        let state = exact_runner::agent::state(p.host().runner());
        assert!(state.contains("/post/9?q=3"), "{platform}: {state}");
        assert!(state.contains("\"fragment\":\"reload\""), "{state}");
        let root = p.host().kernel().roots()[0];
        p.type_text(root, "https://site.invalid/app/post/11?q=4#agent")
            .unwrap();
        let state = exact_runner::agent::state(p.host().runner());
        assert!(state.contains("/post/11?q=4"), "{platform}: {state}");
        assert!(state.contains("\"fragment\":\"agent\""), "{state}");
        for url in [
            "https://other.invalid/app/post/42",
            "https://site.invalid/application/post/42",
            "https://site.invalid:444/app/post/42",
            "custom://app/post/42",
        ] {
            assert!(p.open_location(url, 0.).is_some(), "{platform}: {url}");
        }
    }
}

#[test]
fn linux_delivery() {
    delivery("linux");
}
#[test]
fn windows_delivery() {
    delivery("windows");
}
#[test]
fn android_delivery() {
    delivery("android");
}
