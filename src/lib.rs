#![cfg_attr(not(feature = "std"), no_std)]

use core::ops::Index;

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

#[derive(Encode, Decode, Clone, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct CID<Cid>(Cid);

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
    pub type ManifestCIDOf<T> = BoundedVec<u8, <T as Config>::MaxCID>;
    pub type PoolIdOf<T> = <<T as Config>::Pool as PoolInterface>::PoolId;
    pub type ReplicationFactor = u16;
    pub type Cycles = u16;
    pub type ActiveDays = i32;
    pub type CIDOf<T> = CID<ManifestCIDOf<T>>;
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
            storage: Vec<T::AccountId>,
            pool_id: PoolIdOf<T>,
            manifest: Vec<u8>,
        },
        StorageManifestOutput {
            storer: T::AccountId,
            pool_id: PoolIdOf<T>,
            cid: Vec<u8>,
        },
        RemoveStorerOutput {
            uploader: T::AccountId,
            storage: Option<T::AccountId>,
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
        ManifestAlreadyExist,
        ManifestNotFound,
        ManifestNotStored,
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
            cid: ManifestCIDOf<T>,
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
            cid: ManifestCIDOf<T>,
            pool_id: PoolIdOf<T>,
            replication_factor: ReplicationFactor,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            Self::do_upload_manifest(&who, pool_id, cid, manifest, replication_factor)?;
            Ok(().into())
        }

        #[pallet::weight(10_000)]
        pub fn storage_manifest(
            origin: OriginFor<T>,
            uploader: T::AccountId,
            cid: ManifestCIDOf<T>,
            pool_id: PoolIdOf<T>,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            Self::do_storage_manifest(&who, pool_id, cid)?;
            Ok(().into())
        }

        #[pallet::weight(10_000)]
        pub fn remove_storer(
            origin: OriginFor<T>,
            storage: T::AccountId,
            cid: ManifestCIDOf<T>,
            pool_id: PoolIdOf<T>,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            Self::do_remove_storer(&storage, &who, pool_id, cid)?;
            Ok(().into())
        }

        #[pallet::weight(10_000)]
        pub fn remove_stored_manifest(
            origin: OriginFor<T>,
            uploader: T::AccountId,
            cid: ManifestCIDOf<T>,
            pool_id: PoolIdOf<T>,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            Self::do_remove_storer(&who, &uploader, pool_id, cid)?;
            Ok(().into())
        }

        #[pallet::weight(10_000)]
        pub fn remove_manifest(
            origin: OriginFor<T>,
            cid: ManifestCIDOf<T>,
            pool_id: PoolIdOf<T>,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            Self::do_remove_manifest(&who, pool_id, cid)?;
            Ok(().into())
        }
    }
}

impl<T: Config> Pallet<T> {
    pub fn do_upload_manifest(
        uploader: &T::AccountId,
        pool_id: PoolIdOf<T>,
        cid: ManifestCIDOf<T>,
        manifest: ManifestMetadataOf<T>,
        replication_factor: ReplicationFactor,
    ) -> DispatchResult {
        ensure!(replication_factor > 0, Error::<T>::ReplicationFactorInvalid);
        let mut uploader_vec = Vec::new();
        let mut storers_vec = Vec::new();
        let uploader_data = UploaderData {
            uploader: uploader.clone(),
            storers: storers_vec.to_owned(),
            replication_factor,
        };
        if let Some(manifest) = Self::manifests(pool_id, CID(cid.clone())) {
            Manifests::<T>::try_mutate(pool_id, CID(cid.clone()), |value| -> DispatchResult {
                if let Some(manifest) = value {
                    manifest.users_data.push(uploader_data)
                }
                Ok(())
            })?;
        } else {
            uploader_vec.push(uploader_data);
            Manifests::<T>::insert(
                pool_id,
                CID(cid),
                Manifest {
                    users_data: uploader_vec.to_owned(),
                    manifest_metadata: manifest.clone(),
                },
            );
        }

        Self::deposit_event(Event::ManifestOutput {
            uploader: uploader.clone(),
            storage: storers_vec.to_owned(),
            manifest: manifest.to_vec(),
            pool_id: pool_id.clone(),
        });
        Ok(())
    }

    pub fn do_update_manifest(
        storer: &T::AccountId,
        pool_id: PoolIdOf<T>,
        cid: ManifestCIDOf<T>,
        active_days: ActiveDays,
        active_cycles: Cycles,
        missed_cycles: Cycles,
    ) -> DispatchResult {
        ensure!(
            T::Pool::is_member(storer.clone(), pool_id),
            Error::<T>::AccountNotInPool
        );
        ensure!(
            ManifestsStorerData::<T>::try_get((pool_id, storer.clone(), CID(cid.clone()))).is_ok(),
            Error::<T>::ManifestNotStored
        );
        ManifestsStorerData::<T>::try_mutate(
            (pool_id, storer, CID(cid.clone())),
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
        cid: ManifestCIDOf<T>,
    ) -> DispatchResult {
        ensure!(
            T::Pool::is_member(storer.clone(), pool_id),
            Error::<T>::AccountNotInPool
        );
        ensure!(
            Manifests::<T>::try_get(pool_id, CID(cid.clone())).is_ok(),
            Error::<T>::ManifestNotFound
        );
        Manifests::<T>::try_mutate(pool_id, CID(cid.clone()), |value| -> DispatchResult {
            if let Some(manifest) = value {
                let index = Self::get_uploader_value(manifest.users_data);
                let current_user_data = manifest.users_data.get(index);
                ensure!(
                    current_user_data.is_some(),
                    Error::<T>::ReplicationFactorLimitReached
                );
                ensure!(
                    !current_user_data.unwrap().storers.contains(&storer.clone()),
                    Error::<T>::AccountAlreadyStorer
                );
                manifest
                    .users_data
                    .index(index)
                    .storers
                    .push(storer.clone());
                ManifestsStorerData::<T>::insert(
                    (pool_id, storer, CID(cid.clone())),
                    ManifestStorageData {
                        active_cycles: 0,
                        missed_cycles: 0,
                        active_days: 0,
                    },
                );
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

    pub fn get_uploader_value(data: Vec<UploaderData<T::AccountId>>) -> usize {
        return data
            .iter()
            .position(|x| x.replication_factor - x.storers.len() as u16 > 0)
            .unwrap();
    }

    pub fn do_remove_manifest(
        uploader: &T::AccountId,
        pool_id: PoolIdOf<T>,
        cid: ManifestCIDOf<T>,
    ) -> DispatchResult {
        ensure!(
            Manifests::<T>::try_get(pool_id, CID(cid.clone())).is_ok(),
            Error::<T>::ManifestNotFound
        );
        Manifests::<T>::try_mutate(pool_id, CID(cid.clone()), |value| -> DispatchResult {
            if let Some(manifest) = value {
                for account in manifest.storage.iter() {
                    ManifestsStorerData::<T>::remove((pool_id, account.clone(), CID(cid.clone())));
                }
            }
            Ok(())
        })?;
        Manifests::<T>::remove(pool_id, CID(cid.clone()));

        Self::deposit_event(Event::ManifestRemoved {
            uploader: uploader.clone(),
            cid: cid.to_vec(),
            pool_id: pool_id.clone(),
        });
        Ok(())
    }

    pub fn do_remove_storer(
        storer: &T::AccountId,
        uploader: &T::AccountId,
        pool_id: PoolIdOf<T>,
        cid: ManifestCIDOf<T>,
    ) -> DispatchResult {
        ensure!(
            Manifests::<T>::try_get(pool_id, CID(cid.clone())).is_ok(),
            Error::<T>::ManifestNotFound
        );
        let mut removed_storer = None;
        Manifests::<T>::try_mutate(pool_id, CID(cid.clone()), |value| -> DispatchResult {
            if let Some(manifest) = value {
                ensure!(
                    manifest.storage.contains(storage),
                    Error::<T>::AccountNotStorer
                );
                let value_removed = manifest.storage.swap_remove(
                    manifest
                        .storage
                        .iter()
                        .position(|x| *x == storage.clone())
                        .unwrap(),
                );
                ManifestsStorerData::<T>::remove((
                    pool_id,
                    value_removed.clone(),
                    CID(cid.clone()),
                ));
                removed_storer = Some(value_removed);
            }
            Ok(())
        })?;

        Self::deposit_event(Event::RemoveStorerOutput {
            uploader: uploader.clone(),
            storage: removed_storer,
            cid: cid.to_vec(),
            pool_id: pool_id.clone(),
        });
        Ok(())
    }
}
