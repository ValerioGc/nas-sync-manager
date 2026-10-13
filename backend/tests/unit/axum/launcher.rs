use super::*;
use crate::test_support::{FakeLauncher, Scratch};

#[tokio::test]
async fn says_whether_the_application_works_in_background_and_a_sync_is_on() {
    let scratch = Scratch::new();
    let fake = FakeLauncher::start(&scratch);
    let launcher = fake.launcher();
    launcher.background(true).await;
    launcher.sync_ports(false).await;
    assert_eq!(
        fake.bodies("PUT /api/apps/self/background"),
        [json!({ "needed": true })]
    );
    assert_eq!(
        fake.bodies("PUT /api/apps/self/sync"),
        [json!({ "active": false })]
    );
    // Without the socket nothing is sent.
    let alone = Launcher::new(None);
    alone.background(true).await;
    assert!(alone.request("GET", "/", None).await.is_err());
    // A refusal or a launcher gone only reaches the journal.
    let gone = Launcher::new(Some(scratch.path().join("gone.sock")));
    gone.sync_ports(true).await;
}
