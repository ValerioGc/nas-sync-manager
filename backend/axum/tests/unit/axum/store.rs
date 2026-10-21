use super::*;
use crate::test_support::{database, PC_ID};

fn sync(id: &str) -> Sync {
    Sync {
        id: id.to_owned(),
        device: PC_ID.to_owned(),
        name: "Foto".to_owned(),
        nas_path: "v-a/Foto".to_owned(),
        pc_path: "C:\\Users\\me\\Pictures".to_owned(),
        mode: "two-way".to_owned(),
        paused: false,
        revoked: false,
        created_at: 1,
    }
}

#[tokio::test]
async fn the_settings_start_from_their_defaults_and_keep_what_is_saved() {
    let pool = database().await;
    assert_eq!(settings(&pool).await.unwrap(), Settings::default());
    let changed = Settings {
        max_send_kbps: 500,
        max_recv_kbps: 0,
        rescan_minutes: 30,
    };
    changed.validate().unwrap();
    save_settings(&pool, changed).await.unwrap();
    assert_eq!(settings(&pool).await.unwrap(), changed);
    for wrong in [
        Settings {
            rescan_minutes: 0,
            ..changed
        },
        Settings {
            max_send_kbps: 20_000_000,
            ..changed
        },
    ] {
        assert!(wrong.validate().is_err());
    }
}

#[tokio::test]
async fn a_computer_pairs_again_and_leaves_with_its_syncs() {
    let pool = database().await;
    let device = Device {
        id: PC_ID.to_owned(),
        name: "Portatile".to_owned(),
        client_version: "0.1.0".to_owned(),
        paired_at: 1,
    };
    pair(&pool, &device).await.unwrap();
    pair(
        &pool,
        &Device {
            name: "Portatile nuovo".to_owned(),
            paired_at: 2,
            ..device.clone()
        },
    )
    .await
    .unwrap();
    let paired = super::device(&pool, PC_ID).await.unwrap().unwrap();
    assert_eq!(
        (paired.name.as_str(), paired.paired_at),
        ("Portatile nuovo", 1)
    );

    save_sync(&pool, &sync("nas-a")).await.unwrap();
    let mut changed = sync("nas-a");
    changed.paused = true;
    save_sync(&pool, &changed).await.unwrap();
    save_sync(&pool, &sync("nas-b")).await.unwrap();
    assert_eq!(super::sync(&pool, "nas-a").await.unwrap(), Some(changed));
    assert_eq!(syncs(&pool).await.unwrap().len(), 2);
    assert!(delete_sync(&pool, "nas-b").await.unwrap());
    assert!(!delete_sync(&pool, "nas-b").await.unwrap());

    assert!(unpair(&pool, PC_ID).await.unwrap());
    assert!(syncs(&pool).await.unwrap().is_empty());
    assert!(devices(&pool).await.unwrap().is_empty());
    assert!(!unpair(&pool, PC_ID).await.unwrap());
}
