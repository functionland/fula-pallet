use crate::{mock::*, ManifestMetadataOf, ManifestCIDOf};
use frame_support::assert_ok;
use sp_runtime::traits::Hash;

fn last_event() -> Event {
    frame_system::Pallet::<Test>::events()
        .pop()
        .expect("Event expected")
        .event
}

pub fn before_test() {
    run_to_block(10);
}

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

        let hash = <Test as frame_system::Config>::Hashing::hash(manifest);
        let hash: ManifestMetadataOf<Test> = hash.as_bytes().to_vec().try_into().unwrap();

        let cid = r#"ipfs://QmVzrsZSVJAXkabinxTssvV3xRWyLzWJeQ9rnwyZf5FKoE"#.as_bytes();

        let cid_hash = <Test as frame_system::Config>::Hashing::hash(cid);
        let cid_hash: ManifestCIDOf<Test> = cid_hash.as_bytes().to_vec().try_into().unwrap();

        assert_ok!(Fula::update_manifest(Origin::signed(1), 2, hash.clone(),cid_hash.clone()));

        if let Event::Fula(crate::Event::ManifestUpdated { from, to, manifest }) = last_event() {
            assert_eq!(from, 1);
            assert_eq!(to, 2);
            assert_eq!(manifest.to_vec(), hash.to_vec());
        } else {
            panic!();
        };
    });
}
