use crate::{
    mock::*,
    // SBP-M1 review: remove unused import
    //ManifestCIDOf,
    ManifestMetadataOf,
};
use sp_runtime::traits::Hash;

fn last_event() -> RuntimeEvent {
    frame_system::Pallet::<Test>::events()
        .pop()
        .expect("Event expected")
        .event
}

pub fn before_test() {
    run_to_block(10);
}

// SBP-M1 review: test failure - 'thread 'tests::update_manifest' panicked at 'Event expected', src/tests.rs:11:10'
// SBP-M1 review: add ci to require successful tests before merging
#[test]
fn update_manifest() {
    new_test_ext().execute_with(|| {
        before_test();

        let manifest = r#"
            {
                "job": {
                    "type": "store",
                    "file": "ipfs://QmVzrsZSVJAXkabinxTssvV3xRWyLzWJeQ9rnwyZf5FKoE"
                }
            }
        "#
        .as_bytes();

        // SBP-M1 review: consider type alias to simplify type reference
        let hash = <Test as frame_system::Config>::Hashing::hash(manifest);
        let hash: ManifestMetadataOf<Test> = hash.as_bytes().to_vec().try_into().unwrap();

        // SBP-M1 review: remove commented out code
        // let cid = r#"ipfs://QmVzrsZSVJAXkabinxTssvV3xRWyLzWJeQ9rnwyZf5FKoE"#.as_bytes();

        // let cid_hash = <Test as frame_system::Config>::Hashing::hash(cid);
        // let cid_hash: ManifestCIDOf<Test> = cid_hash.as_bytes().to_vec().try_into().unwrap();

        if let RuntimeEvent::Fula(crate::Event::ManifestOutput {
            uploader,
            storer,
            manifest,
            pool_id,
        }) = last_event()
        {
            assert_eq!(uploader, 1);
            assert_eq!(storer, vec![2]);
            assert_eq!(manifest.to_vec(), hash.to_vec());
            assert_eq!(pool_id, 1);
        } else {
            panic!();
        };
    });
}
