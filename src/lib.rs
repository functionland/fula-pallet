#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{dispatch::DispatchResult, ensure, traits::Get, BoundedVec};
use fula_pool::PoolInterface;
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;
use sp_std::prelude::*;

/// Edit this file to define custom logic or remove it if it is not needed.
/// Learn more about FRAME and the core library of Substrate FRAME pallets:
/// <https://docs.substrate.io/v3/runtime/frame>
pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[derive(Encode, Decode, Clone, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct Manifest<AccountId, ManifestMetadataOf> {
    pub users_data: Vec<UploaderData<AccountId>>,
    pub manifest_metadata: ManifestMetadataOf,
}

#[derive(Encode, Decode, Clone, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct UploaderData<AccountId> {
    pub uploader: AccountId,
    pub storers: Vec<AccountId>,
    pub replication_factor: ReplicationFactor,
}

#[derive(Encode, Decode, Clone, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct ManifestStorageData {
    pub active_cycles: Cycles,
    pub missed_cycles: Cycles,
    pub active_days: ActiveDays,
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{dispatch::DispatchResultWithPostInfo, pallet_prelude::*};
    use frame_system::pallet_prelude::*;

    /// Configure the pallet by specifying the parameters and types on which it depends.
    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        #[pallet::constant]
        type MaxManifestMetadata: Get<u32>;
        type MaxCID: Get<u32>;
        type Pool: PoolInterface<AccountId = Self::AccountId>;
    }

    pub type ManifestMetadataOf<T> = BoundedVec<u8, <T as Config>::MaxManifestMetadata>;
    pub type CIDOf<T> = BoundedVec<u8, <T as Config>::MaxCID>;
    pub type PoolIdOf<T> = <<T as Config>::Pool as PoolInterface>::PoolId;
    pub type ReplicationFactor = u16;
    pub type Cycles = u16;
    pub type ActiveDays = i32;
    pub type ManifestOf<T> =
        Manifest<<T as frame_system::Config>::AccountId, ManifestMetadataOf<T>>;

    #[pallet::pallet]
    #[pallet::without_storage_info]
    #[pallet::generate_store(pub(super) trait Store)]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    #[pallet::getter(fn manifests)]
    pub(super) type Manifests<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        PoolIdOf<T>,
        Blake2_128Concat,
        CIDOf<T>,
        ManifestOf<T>,
    >;

    #[pallet::storage]
    #[pallet::getter(fn manifests_storage_data)]
    pub(super) type ManifestsStorerData<T: Config> = StorageNMap<
        _,
        (
            NMapKey<Blake2_128Concat, PoolIdOf<T>>,
            NMapKey<Blake2_128Concat, T::AccountId>,
            NMapKey<Blake2_128Concat, CIDOf<T>>,
        ),
        ManifestStorageData,
    >;

    // Pallets use events to inform users when important changes are made.
    // https://docs.substrate.io/v3/runtime/events-and-errors
    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        ManifestOutput {
            uploader: T::AccountId,
            storer: Vec<T::AccountId>,
            pool_id: PoolIdOf<T>,
            manifest: Vec<u8>,
        },
        StorageManifestOutput {
            storer: T::AccountId,
            pool_id: PoolIdOf<T>,
            cid: Vec<u8>,
        },
        RemoveStorerOutput {
            storer: Option<T::AccountId>,
            pool_id: PoolIdOf<T>,
            cid: Vec<u8>,
        },
        ManifestRemoved {
            uploader: T::AccountId,
            pool_id: PoolIdOf<T>,
            cid: Vec<u8>,
        },
        ManifestStorageUpdated {
            storer: T::AccountId,
            pool_id: PoolIdOf<T>,
            cid: Vec<u8>,
            active_cycles: Cycles,
            missed_cycles: Cycles,
            active_days: ActiveDays,
        },
        BatchManifestOutput {
            uploader: T::AccountId,
            pool_ids: Vec<PoolIdOf<T>>,
            manifests: Vec<Vec<u8>>,
        },
        BatchStorageManifestOutput {
            storer: T::AccountId,
            pool_id: PoolIdOf<T>,
            cids: Vec<Vec<u8>>,
        },
        BatchRemoveStorerOutput {
            storer: T::AccountId,
            pool_id: PoolIdOf<T>,
            cids: Vec<Vec<u8>>,
        },
        BatchManifestRemoved {
            uploader: T::AccountId,
            pool_ids: Vec<PoolIdOf<T>>,
            cids: Vec<Vec<u8>>,
        },
        VerifiedStorerManifests {
            storer: T::AccountId,
            valid_cids: Vec<Vec<u8>>,
            invalid_cids: Vec<Vec<u8>>,
        },
    }

    // Errors inform users that something went wrong.
    #[pallet::error]
    pub enum Error<T> {
        NoneValue,
        StorageOverflow,
        ReplicationFactorLimitReached,
        ReplicationFactorInvalid,
        AccountAlreadyStorer,
        AccountNotStorer,
        AccountNotInPool,
        AccountNotUploader,
        ManifestAlreadyExist,
        ManifestNotFound,
        ManifestNotStored,
        InvalidArrayLength,
    }

    // Dispatchable functions allows users to interact with the pallet and invoke state changes.
    // These functions materialize as "extrinsics", which are often compared to transactions.
    // Dispatchable functions must be annotated with a weight and must return a DispatchResult.
    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Updates fula manifest uploader to
        #[pallet::weight(10_000)]
        pub fn update_manifest(
            origin: OriginFor<T>,
            cid: CIDOf<T>,
            pool_id: PoolIdOf<T>,
            active_cycles: Cycles,
            missed_cycles: Cycles,
            active_days: ActiveDays,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            Self::do_update_manifest(
                &who,
                pool_id,
                cid,
                active_days,
                active_cycles,
                missed_cycles,
            )?;
            Ok(().into())
        }

        #[pallet::weight(10_000)]
        pub fn upload_manifest(
            origin: OriginFor<T>,
            manifest: ManifestMetadataOf<T>,
            cid: CIDOf<T>,
            pool_id: PoolIdOf<T>,
            replication_factor: ReplicationFactor,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            Self::do_upload_manifest(&who, pool_id, cid, manifest, replication_factor)?;
            Ok(().into())
        }

        #[pallet::weight(10_000)]
        pub fn batch_upload_manifest(
            origin: OriginFor<T>,
            manifest: Vec<ManifestMetadataOf<T>>,
            cids: Vec<CIDOf<T>>,
            pool_id: Vec<PoolIdOf<T>>,
            replication_factor: Vec<ReplicationFactor>,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            Self::do_batch_upload_manifest(&who, pool_id, cids, manifest, replication_factor)?;
            Ok(().into())
        }

        #[pallet::weight(10_000)]
        pub fn storage_manifest(
            origin: OriginFor<T>,
            cid: CIDOf<T>,
            pool_id: PoolIdOf<T>,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            Self::do_storage_manifest(&who, pool_id, cid)?;
            Ok(().into())
        }

        #[pallet::weight(10_000)]
        pub fn batch_storage_manifest(
            origin: OriginFor<T>,
            cids: Vec<CIDOf<T>>,
            pool_id: PoolIdOf<T>,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            Self::do_batch_storage_manifest(&who, pool_id, cids)?;
            Ok(().into())
        }

        #[pallet::weight(10_000)]
        pub fn remove_stored_manifest(
            origin: OriginFor<T>,
            cid: CIDOf<T>,
            pool_id: PoolIdOf<T>,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            Self::do_remove_storer(&who, pool_id, cid)?;
            Ok(().into())
        }

        #[pallet::weight(10_000)]
        pub fn batch_remove_stored_manifest(
            origin: OriginFor<T>,
            cids: Vec<CIDOf<T>>,
            pool_id: PoolIdOf<T>,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            Self::do_batch_remove_storer(&who, pool_id, cids)?;
            Ok(().into())
        }

        #[pallet::weight(10_000)]
        pub fn remove_manifest(
            origin: OriginFor<T>,
            cid: CIDOf<T>,
            pool_id: PoolIdOf<T>,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            Self::do_remove_manifest(&who, pool_id, cid)?;
            Ok(().into())
        }

        #[pallet::weight(10_000)]
        pub fn batch_remove_manifest(
            origin: OriginFor<T>,
            cids: Vec<CIDOf<T>>,
            pool_ids: Vec<PoolIdOf<T>>,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            Self::do_batch_remove_manifest(&who, pool_ids, cids)?;
            Ok(().into())
        }

        #[pallet::weight(10_000)]
        pub fn verify_manifests(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            Self::do_verify_manifests(&who)?;
            Ok(().into())
        }
    }
}

impl<T: Config> Pallet<T> {
    pub fn do_upload_manifest(
        uploader: &T::AccountId,
        pool_id: PoolIdOf<T>,
        cid: CIDOf<T>,
        manifest: ManifestMetadataOf<T>,
        replication_factor: ReplicationFactor,
    ) -> DispatchResult {
        ensure!(replication_factor > 0, Error::<T>::ReplicationFactorInvalid);
        let mut uploader_vec = Vec::new();
        let storers_vec = Vec::new();
        let uploader_data = UploaderData {
            uploader: uploader.clone(),
            storers: storers_vec.to_owned(),
            replication_factor,
        };
        if let Some(_manifest) = Self::manifests(pool_id, cid.clone()) {
            Manifests::<T>::try_mutate(pool_id, cid.clone(), |value| -> DispatchResult {
                if let Some(manifest) = value {
                    manifest.users_data.push(uploader_data)
                }
                Ok(())
            })?;
        } else {
            uploader_vec.push(uploader_data);
            Manifests::<T>::insert(
                pool_id,
                cid,
                Manifest {
                    users_data: uploader_vec.to_owned(),
                    manifest_metadata: manifest.clone(),
                },
            );
        }

        Self::deposit_event(Event::ManifestOutput {
            uploader: uploader.clone(),
            storer: storers_vec.to_owned(),
            manifest: manifest.to_vec(),
            pool_id: pool_id.clone(),
        });
        Ok(())
    }

    pub fn do_verify_manifests(storer: &T::AccountId) -> DispatchResult {
        let mut invalid_cids = Vec::new();
        let mut valid_cids = Vec::new();

        for item in ManifestsStorerData::<T>::iter() {
            if storer.clone() == item.0 .1.clone() {
                if T::Pool::is_member(item.0 .1.clone(), item.0 .0) {
                    if Manifests::<T>::try_get(item.0 .0, item.0 .2.clone()).is_ok() {
                        valid_cids.push(item.0 .2.to_vec());
                    } else {
                        invalid_cids.push(item.0 .2.to_vec());
                        ManifestsStorerData::<T>::remove((
                            item.0 .0,
                            item.0 .1.clone(),
                            item.0 .2.clone(),
                        ));
                    }
                } else {
                    invalid_cids.push(item.0 .2.to_vec());
                    ManifestsStorerData::<T>::remove((
                        item.0 .0,
                        item.0 .1.clone(),
                        item.0 .2.clone(),
                    ));
                }
            }
        }

        Self::deposit_event(Event::VerifiedStorerManifests {
            storer: storer.clone(),
            valid_cids: valid_cids.to_vec(),
            invalid_cids: invalid_cids.to_vec(),
        });
        Ok(())
    }

    pub fn do_batch_upload_manifest(
        uploader: &T::AccountId,
        pool_ids: Vec<PoolIdOf<T>>,
        cids: Vec<CIDOf<T>>,
        manifests: Vec<ManifestMetadataOf<T>>,
        replication_factors: Vec<ReplicationFactor>,
    ) -> DispatchResult {
        ensure!(
            cids.len() == manifests.len(),
            Error::<T>::InvalidArrayLength
        );
        ensure!(
            cids.len() == replication_factors.len(),
            Error::<T>::InvalidArrayLength
        );

        let n = cids.len();
        for i in 0..n {
            let cid = cids[i].to_owned();
            let manifest = manifests[i].to_owned();
            let replication_factor = replication_factors[i];
            let pool_id = pool_ids[i];
            Self::do_upload_manifest(uploader, pool_id, cid, manifest, replication_factor)?;
        }

        Self::deposit_event(Event::BatchManifestOutput {
            uploader: uploader.clone(),
            manifests: Self::transform_manifest_to_vec(manifests),
            pool_ids: pool_ids.clone(),
        });
        Ok(())
    }

    pub fn do_update_manifest(
        storer: &T::AccountId,
        pool_id: PoolIdOf<T>,
        cid: CIDOf<T>,
        active_days: ActiveDays,
        active_cycles: Cycles,
        missed_cycles: Cycles,
    ) -> DispatchResult {
        ensure!(
            T::Pool::is_member(storer.clone(), pool_id),
            Error::<T>::AccountNotInPool
        );
        ensure!(
            ManifestsStorerData::<T>::try_get((pool_id, storer.clone(), cid.clone())).is_ok(),
            Error::<T>::ManifestNotStored
        );
        ManifestsStorerData::<T>::try_mutate(
            (pool_id, storer, cid.clone()),
            |value| -> DispatchResult {
                if let Some(manifest) = value {
                    manifest.active_cycles = active_cycles;
                    manifest.missed_cycles = missed_cycles;
                    manifest.active_days = active_days;
                }
                Ok(())
            },
        )?;

        Self::deposit_event(Event::ManifestStorageUpdated {
            storer: storer.clone(),
            pool_id: pool_id.clone(),
            cid: cid.to_vec(),
            active_cycles,
            missed_cycles,
            active_days,
        });
        Ok(())
    }

    pub fn do_storage_manifest(
        storer: &T::AccountId,
        pool_id: PoolIdOf<T>,
        cid: CIDOf<T>,
    ) -> DispatchResult {
        ensure!(
            T::Pool::is_member(storer.clone(), pool_id),
            Error::<T>::AccountNotInPool
        );
        ensure!(
            Manifests::<T>::try_get(pool_id, cid.clone()).is_ok(),
            Error::<T>::ManifestNotFound
        );
        Manifests::<T>::try_mutate(pool_id, cid.clone(), |value| -> DispatchResult {
            if let Some(manifest) = value {
                if let Some(index) = Self::get_uploader_value(manifest.users_data.to_vec()) {
                    ensure!(
                        !manifest.users_data[index].storers.contains(&storer.clone()),
                        Error::<T>::AccountAlreadyStorer
                    );
                    manifest.users_data[index].storers.push(storer.clone());
                    ManifestsStorerData::<T>::insert(
                        (pool_id, storer, cid.clone()),
                        ManifestStorageData {
                            active_cycles: 0,
                            missed_cycles: 0,
                            active_days: 0,
                        },
                    );
                }
            }
            Ok(())
        })?;

        Self::deposit_event(Event::StorageManifestOutput {
            storer: storer.clone(),
            pool_id: pool_id.clone(),
            cid: cid.to_vec(),
        });
        Ok(())
    }

    pub fn do_batch_storage_manifest(
        storer: &T::AccountId,
        pool_id: PoolIdOf<T>,
        cids: Vec<CIDOf<T>>,
    ) -> DispatchResult {
        let n = cids.len();
        for i in 0..n {
            let cid = cids[i].to_owned();
            Self::do_storage_manifest(storer, pool_id, cid)?;
        }

        Self::deposit_event(Event::BatchStorageManifestOutput {
            storer: storer.clone(),
            cids: Self::transform_cid_to_vec(cids),
            pool_id: pool_id.clone(),
        });
        Ok(())
    }

    pub fn do_remove_manifest(
        uploader: &T::AccountId,
        pool_id: PoolIdOf<T>,
        cid: CIDOf<T>,
    ) -> DispatchResult {
        ensure!(
            Manifests::<T>::try_get(pool_id, cid.clone()).is_ok(),
            Error::<T>::ManifestNotFound
        );

        let manifest = Self::manifests(pool_id, cid.clone()).unwrap();

        if let Some(index) =
            Self::get_uploader_index(manifest.users_data.to_owned(), uploader.clone())
        {
            if manifest.users_data.to_owned().len() > 1 {
                Manifests::<T>::try_mutate(pool_id, cid.clone(), |value| -> DispatchResult {
                    if let Some(manifest) = value {
                        manifest.users_data.remove(index);
                    }
                    Ok(())
                })?;
            } else {
                Manifests::<T>::remove(pool_id, cid.clone());
            }
            // for account in manifest.users_data[index].storers.iter() {
            //     ManifestsStorerData::<T>::remove((pool_id, account.clone(), cid.clone()));
            // }
        }

        Self::deposit_event(Event::ManifestRemoved {
            uploader: uploader.clone(),
            cid: cid.to_vec(),
            pool_id: pool_id.clone(),
        });
        Ok(())
    }

    pub fn do_batch_remove_manifest(
        uploader: &T::AccountId,
        pool_ids: Vec<PoolIdOf<T>>,
        cids: Vec<CIDOf<T>>,
    ) -> DispatchResult {
        ensure!(cids.len() == pool_ids.len(), Error::<T>::InvalidArrayLength);

        let n = cids.len();
        for i in 0..n {
            let cid = cids[i].to_owned();
            let pool_id = pool_ids[i];
            Self::do_remove_manifest(uploader, pool_id, cid)?;
        }

        Self::deposit_event(Event::BatchManifestRemoved {
            uploader: uploader.clone(),
            cids: Self::transform_cid_to_vec(cids),
            pool_ids: pool_ids.clone(),
        });
        Ok(())
    }

    pub fn do_remove_storer(
        storer: &T::AccountId,
        pool_id: PoolIdOf<T>,
        cid: CIDOf<T>,
    ) -> DispatchResult {
        ensure!(
            Manifests::<T>::try_get(pool_id, cid.clone()).is_ok(),
            Error::<T>::ManifestNotFound
        );
        let mut removed_storer = None;
        Manifests::<T>::try_mutate(pool_id, cid.clone(), |value| -> DispatchResult {
            if let Some(manifest) = value {
                ensure!(
                    Self::verify_storer_contained(manifest.users_data.to_owned(), storer).is_some(),
                    Error::<T>::AccountNotStorer
                );
                if let Some(uploader_index) =
                    Self::verify_storer_contained(manifest.users_data.to_owned(), storer)
                {
                    if let Some(storer_index) = Self::verify_account_in_storers(
                        manifest.users_data.to_owned(),
                        uploader_index,
                        storer.clone(),
                    ) {
                        let value_removed = manifest.users_data[uploader_index]
                            .storers
                            .remove(storer_index);
                        // ManifestsStorerData::<T>::remove((
                        //     pool_id,
                        //     value_removed.clone(),
                        //     CID(cid.clone()),
                        // ));
                        removed_storer = Some(value_removed);
                    }
                };
            }
            Ok(())
        })?;

        Self::deposit_event(Event::RemoveStorerOutput {
            storer: removed_storer,
            cid: cid.to_vec(),
            pool_id: pool_id.clone(),
        });
        Ok(())
    }

    pub fn do_batch_remove_storer(
        storer: &T::AccountId,
        pool_id: PoolIdOf<T>,
        cids: Vec<CIDOf<T>>,
    ) -> DispatchResult {
        let n = cids.len();
        for i in 0..n {
            let cid = cids[i].to_owned();
            Self::do_remove_storer(storer, pool_id, cid)?;
        }

        Self::deposit_event(Event::BatchRemoveStorerOutput {
            storer: storer.clone(),
            cids: Self::transform_cid_to_vec(cids),
            pool_id: pool_id.clone(),
        });
        Ok(())
    }

    pub fn get_uploader_value(data: Vec<UploaderData<T::AccountId>>) -> Option<usize> {
        return data
            .iter()
            .position(|x| x.replication_factor - x.storers.len() as u16 > 0);
    }

    pub fn get_uploader_index(
        data: Vec<UploaderData<T::AccountId>>,
        account: T::AccountId,
    ) -> Option<usize> {
        return data.iter().position(|x| x.uploader == account);
    }
    pub fn verify_storer_contained(
        data: Vec<UploaderData<T::AccountId>>,
        account: &T::AccountId,
    ) -> Option<usize> {
        return data.iter().position(|x| x.storers.contains(account));
    }

    pub fn verify_account_in_storers(
        data: Vec<UploaderData<T::AccountId>>,
        index: usize,
        account: T::AccountId,
    ) -> Option<usize> {
        return data
            .get(index)
            .unwrap()
            .storers
            .iter()
            .position(|x| *x == account.clone());
    }

    fn transform_manifest_to_vec(in_vec: Vec<ManifestMetadataOf<T>>) -> Vec<Vec<u8>> {
        in_vec
            .into_iter()
            .map(|manifest| manifest.to_vec())
            .collect()
    }

    fn transform_cid_to_vec(in_vec: Vec<CIDOf<T>>) -> Vec<Vec<u8>> {
        in_vec.into_iter().map(|cid| cid.to_vec()).collect()
    }
}
