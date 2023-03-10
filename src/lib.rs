#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{dispatch::DispatchResult, ensure, traits::Get, BoundedVec};
use fula_pool::PoolInterface;
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;
use sp_std::prelude::*;
use frame_system::{self as system};
use sp_std::vec::Vec;
use sp_runtime::traits::{Hash};
use sp_core::blake2_256;

/// Edit this file to define custom logic or remove it if it is not needed.
/// Learn more about FRAME and the core library of Substrate FRAME pallets:
/// <https://docs.substrate.io/v3/runtime/frame>
pub use pallet::*;

pub trait MaxRange {
    type Range;
    fn random(max_range: Self::Range) -> u64;
}

pub type Range = u64;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

// Main structs for the manifests

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

// Manifests structs for the Get calls

#[derive(Encode, Decode, Clone, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct ManifestWithPoolId<PoolId, AccountId, ManifestMetadataOf> {
    pub pool_id: PoolId,
    pub users_data: Vec<UploaderData<AccountId>>,
    pub manifest_metadata: ManifestMetadataOf,
}

#[derive(Encode, Decode, Clone, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct ManifestAvailable<PoolId, ManifestMetadataOf> {
    pub pool_id: PoolId,
    pub replication_factor: ReplicationFactor,
    pub manifest_metadata: ManifestMetadataOf,
}

#[derive(Encode, Decode, Clone, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct StorerData<PoolId, Cid, AccountId> {
    pub pool_id: PoolId,
    pub cid: Cid,
    pub account: AccountId,
    pub manifest_data: ManifestStorageData,
}

// Challenge Structs

#[derive(Encode, Decode, Clone, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct Challenge<AccountId> {
    pub challenger: AccountId,
    pub challenge_state: ChallengeState,
}

#[derive(Encode, Decode, Clone, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum ChallengeState {
    Open,
    Successful,
    Failed,
}

// Manifests storer data structs

#[derive(Encode, Decode, Clone, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct ManifestStorageData {
    pub active_cycles: Cycles,
    pub missed_cycles: Cycles,
    pub active_days: ActiveDays,
    pub challenge_state: ChallengeState,
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
    pub type ManifestWithPoolIdOf<T> = ManifestWithPoolId<
        PoolIdOf<T>,
        <T as frame_system::Config>::AccountId,
        ManifestMetadataOf<T>,
    >;
    pub type ManifestAvailableOf<T> = ManifestAvailable<PoolIdOf<T>, ManifestMetadataOf<T>>;
    pub type StorerDataOf<T> =
        StorerData<PoolIdOf<T>, CIDOf<T>, <T as frame_system::Config>::AccountId>;
    pub type ChallengeRequestsOf<T> = Challenge<<T as frame_system::Config>::AccountId>;

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
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn challenges)]
    pub(super) type ChallengeRequests<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        CIDOf<T>,
        ChallengeRequestsOf<T>,
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
        GetManifests {
            manifests: Vec<ManifestWithPoolIdOf<T>>,
        },
        GetAvailableManifests {
            manifests: Vec<ManifestAvailableOf<T>>,
        },
        GetManifestsStorerData {
            manifests: Vec<StorerDataOf<T>>,
        },
        Challenge {
            challenger: T::AccountId,
            challenged: T::AccountId,
            cid: Vec<u8>,
            state: ChallengeState,
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
        ErrorPickingCIDToChallenge,
        ErrorPickingAccountToChallenge,
        ManifestStorerDataNotFound,
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

        #[pallet::weight(10_000)]
        pub fn get_manifests(
            _origin: OriginFor<T>,
            pool_id: Option<PoolIdOf<T>>,
            uploader: Option<T::AccountId>,
            storer: Option<T::AccountId>,
        ) -> DispatchResultWithPostInfo {
            Self::do_get_manifests(pool_id, uploader, storer)?;
            Ok(().into())
        }

        #[pallet::weight(10_000)]
        pub fn get_available_manifests(
            _origin: OriginFor<T>,
            pool_id: Option<PoolIdOf<T>>,
        ) -> DispatchResultWithPostInfo {
            Self::do_get_available_manifests(pool_id)?;
            Ok(().into())
        }

        #[pallet::weight(10_000)]
        pub fn get_manifests_storer_data(
            _origin: OriginFor<T>,
            pool_id: Option<PoolIdOf<T>>,
            storer: Option<T::AccountId>,
        ) -> DispatchResultWithPostInfo {
            Self::do_get_manifest_storer_data(pool_id, storer)?;
            Ok(().into())
        }

        #[pallet::weight(10_000)]
        pub fn generate_challenge(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            
            Self::do_generate_challenge(&who)?;
            Ok(().into())
        }

        #[pallet::weight(10_000)]
        pub fn verify_challenge(
            origin: OriginFor<T>,
            pool_id: PoolIdOf<T>,
            cids: Vec<CIDOf<T>>,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            Self::do_verify_challenge(&who, cids, pool_id)?;
            Ok(().into())
        }
    }

    impl<T: Config> MaxRange for Pallet<T> {
        type Range = Range;
            fn random(max_range: Self::Range) -> u64 {
            //let who = system::ensure_signed(origin)?;
            let block_number = <system::Pallet<T>>::block_number();
            
            let mut input = Vec::new();
            //input.extend_from_slice(&who.encode());
            input.extend_from_slice(&block_number.encode());
            
            let hash_result = blake2_256(&input);
            
            let random_number = T::Hashing::hash(&hash_result);
            
            //let max_range = 100; //change to any desired range
            
            let result = random_number.as_ref()
                .iter()
                .take(4) // take only the first 4 bytes
                .fold(0, |acc, &byte| (acc << 8) + byte as u32)
                % max_range as u32;
            
            return result as u64;
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
                if let Some(index) =
                    Self::get_next_available_uploader_index(manifest.users_data.to_vec())
                {
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
                            challenge_state: ChallengeState::Open,
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
                    Self::verify_account_is_storer(manifest.users_data.to_owned(), storer),
                    Error::<T>::AccountNotStorer
                );
                if let Some(uploader_index) =
                    Self::get_uploader_index_given_storer(manifest.users_data.to_owned(), storer)
                {
                    if let Some(storer_index) = Self::get_storer_index(
                        manifest.users_data.to_owned(),
                        uploader_index,
                        storer.clone(),
                    ) {
                        let value_removed = manifest.users_data[uploader_index]
                            .storers
                            .remove(storer_index);
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

    pub fn do_get_manifests(
        pool_id: Option<PoolIdOf<T>>,
        uploader: Option<T::AccountId>,
        storer: Option<T::AccountId>,
    ) -> DispatchResult {
        let mut manifests_result = Vec::new();

        for item in Manifests::<T>::iter() {
            let mut meet_requirements = true;

            if let Some(pool_id_value) = pool_id {
                if pool_id_value != item.0 {
                    meet_requirements = false;
                }
            }

            if let Some(ref uploader_value) = uploader {
                if Self::verify_account_is_uploader(
                    item.2.users_data.to_vec(),
                    uploader_value.clone(),
                ) {
                    meet_requirements = false;
                }
            }

            if let Some(ref storer_value) = storer {
                if Self::verify_account_is_storer(item.2.users_data.to_vec(), storer_value) {
                    meet_requirements = false;
                }
            }

            if meet_requirements {
                manifests_result.push(ManifestWithPoolId {
                    pool_id: item.0,
                    users_data: item.2.users_data,
                    manifest_metadata: item.2.manifest_metadata,
                });
            }
        }
        Self::deposit_event(Event::GetManifests {
            manifests: manifests_result,
        });
        Ok(())
    }

    pub fn do_get_available_manifests(pool_id: Option<PoolIdOf<T>>) -> DispatchResult {
        let mut manifests_result = Vec::new();

        for item in Manifests::<T>::iter() {
            let mut meet_requirements = true;

            if let Some(_) = Self::get_next_available_uploader_index(item.2.users_data.to_vec()) {
                if let Some(pool_id_value) = pool_id {
                    if pool_id_value != item.0 {
                        meet_requirements = false;
                    }
                }

                if meet_requirements {
                    manifests_result.push(ManifestAvailable {
                        pool_id: item.0,
                        manifest_metadata: item.2.manifest_metadata,
                        replication_factor: Self::get_added_replication_factor(
                            item.2.users_data.to_vec(),
                        ),
                    });
                }
            }
        }
        Self::deposit_event(Event::GetAvailableManifests {
            manifests: manifests_result,
        });
        Ok(())
    }

    pub fn do_get_manifest_storer_data(
        pool_id: Option<PoolIdOf<T>>,
        storer: Option<T::AccountId>,
    ) -> DispatchResult {
        let mut manifests_result = Vec::new();

        for item in ManifestsStorerData::<T>::iter() {
            let mut meet_requirements = true;

            if let Some(pool_id_value) = pool_id {
                if pool_id_value != item.0 .0 {
                    meet_requirements = false;
                }
            }

            if let Some(ref storer_value) = storer {
                if storer_value.clone() != item.0 .1.clone() {
                    meet_requirements = false;
                }
            }

            if meet_requirements {
                manifests_result.push(StorerData {
                    pool_id: item.0 .0,
                    cid: item.0 .2,
                    account: item.0 .1.clone(),
                    manifest_data: ManifestStorageData {
                        active_cycles: item.1.active_cycles,
                        missed_cycles: item.1.missed_cycles,
                        active_days: item.1.active_days,
                        challenge_state: item.1.challenge_state,
                    },
                });
            }
        }
        Self::deposit_event(Event::GetManifestsStorerData {
            manifests: manifests_result,
        });
        Ok(())
    }

    pub fn pick_random_account_cid_pair() -> (Option<T::AccountId>, Option<CIDOf<T>>) {
        let max_value = ManifestsStorerData::<T>::iter().count();
        let random_value = <pallet::Pallet<T> as MaxRange>::random(max_value as u64);

        if let Some(item) = ManifestsStorerData::<T>::iter().nth(random_value.try_into().unwrap()) {
            let account = Some(item.0 .1);
            let cid = Some(item.0 .2);

            return (account, cid);
        } else {
            return (None, None);
        }
    }

    pub fn do_generate_challenge(challenger: &T::AccountId) -> DispatchResult {
        let pair = Self::pick_random_account_cid_pair();
        ensure!(pair.0.is_some(), Error::<T>::ErrorPickingAccountToChallenge);
        ensure!(pair.1.is_some(), Error::<T>::ErrorPickingCIDToChallenge);

        Self::deposit_event(Event::Challenge {
            challenger: challenger.clone(),
            challenged: pair.0.unwrap().clone(),
            cid: pair.1.unwrap().to_vec(),
            state: ChallengeState::Open,
        });
        Ok(())
    }

    pub fn remove_challenge(
        who: &T::AccountId,
        cid: CIDOf<T>,
        pool: PoolIdOf<T>,
        mut data: ManifestStorageData,
        state: ChallengeState,
    ) {
        ChallengeRequests::<T>::remove(who, cid.clone());
        data.challenge_state = state;
        ManifestsStorerData::<T>::set((pool, who, cid.clone()), Some(data));
    }

    pub fn do_verify_challenge(
        challenged: &T::AccountId,
        cids: Vec<CIDOf<T>>,
        pool_id: PoolIdOf<T>,
    ) -> DispatchResult {
        ensure!(
            T::Pool::is_member(challenged.clone(), pool_id),
            Error::<T>::AccountNotInPool
        );
        for item in ChallengeRequests::<T>::iter() {
            if item.0.clone() == challenged.clone() {
                let state;
                if cids.contains(&item.1) {
                    state = ChallengeState::Successful
                } else {
                    state = ChallengeState::Failed
                }
                let manifest =
                    Self::manifests_storage_data((pool_id, challenged.clone(), item.1.clone()))
                        .ok_or(Error::<T>::ManifestStorerDataNotFound)?;
                Self::remove_challenge(challenged, item.1, pool_id, manifest, state);
            }
        }
        Ok(())
    }

    pub fn get_next_available_uploader_index(
        data: Vec<UploaderData<T::AccountId>>,
    ) -> Option<usize> {
        return data
            .iter()
            .position(|x| x.replication_factor - x.storers.len() as u16 > 0);
    }

    pub fn get_added_replication_factor(data: Vec<UploaderData<T::AccountId>>) -> u16 {
        let mut result = 0;
        for user_data in data {
            result += user_data.replication_factor - user_data.storers.len() as u16;
        }
        return result.into();
    }

    pub fn get_uploader_index(
        data: Vec<UploaderData<T::AccountId>>,
        account: T::AccountId,
    ) -> Option<usize> {
        return data.iter().position(|x| x.uploader == account);
    }

    pub fn verify_account_is_uploader(
        data: Vec<UploaderData<T::AccountId>>,
        account: T::AccountId,
    ) -> bool {
        return data.iter().position(|x| x.uploader == account).is_some();
    }

    pub fn get_uploader_index_given_storer(
        data: Vec<UploaderData<T::AccountId>>,
        account: &T::AccountId,
    ) -> Option<usize> {
        return data.iter().position(|x| x.storers.contains(account));
    }

    pub fn verify_account_is_storer(
        data: Vec<UploaderData<T::AccountId>>,
        account: &T::AccountId,
    ) -> bool {
        return data
            .iter()
            .position(|x| x.storers.contains(account))
            .is_some();
    }

    pub fn get_storer_index(
        data: Vec<UploaderData<T::AccountId>>,
        uploader_index: usize,
        account: T::AccountId,
    ) -> Option<usize> {
        return data[uploader_index]
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
