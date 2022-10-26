#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{dispatch::DispatchResult, traits::Get, BoundedVec, ensure};
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
    pub storage: Vec<AccountId>,
    pub replication_factor: u16,
    pub manifest_data: ManifestData<AccountId,ManifestMetadataOf>,
}

#[derive(Encode, Decode, Clone, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct ManifestData<AccountId, ManifestMetadataOf> {
    pub uploader: AccountId,
    pub manifest_metadata: ManifestMetadataOf,
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
        type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

        #[pallet::constant]
        type MaxManifestMetadata: Get<u32>;
        type MaxCID: Get<u32>;
    }

    pub type ManifestMetadataOf<T> = BoundedVec<u8, <T as Config>::MaxManifestMetadata>;
    pub type ManifestCIDOf<T> = BoundedVec<u8, <T as Config>::MaxCID>;
    pub type PoolID = u16;
    pub type CIDOf<T> = CID<ManifestCIDOf<T>>;
    pub type ManifestOf<T> = Manifest<<T as frame_system::Config>::AccountId, ManifestMetadataOf<T>>;

    #[pallet::pallet]
    #[pallet::without_storage_info]
    #[pallet::generate_store(pub(super) trait Store)]
    pub struct Pallet<T>(_);

    // The pallet's runtime storage items.
    // https://docs.substrate.io/v3/runtime/storage
    #[pallet::storage]
    #[pallet::getter(fn something)]
    // Learn more about declaring storage items:
    // https://docs.substrate.io/v3/runtime/storage#declaring-storage-items
    pub type Something<T> = StorageValue<_, u32>;

    #[pallet::storage]
    #[pallet::getter(fn manifests)]
    pub(super) type Manifests<T: Config> = StorageNMap<
        _,
        (
            NMapKey<Blake2_128Concat, PoolID>,
            NMapKey<Blake2_128Concat, T::AccountId>,
            NMapKey<Blake2_128Concat, CIDOf<T>>,
        ),
        ManifestOf<T>
    >;

    // Pallets use events to inform users when important changes are made.
    // https://docs.substrate.io/v3/runtime/events-and-errors
    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        ManifestOutput {
            uploader: T::AccountId,
            storage: Vec<T::AccountId>,
            manifest: Vec<u8>,
            pool_id: u16,
        },
        StorageManifestOutput {
            uploader: T::AccountId,
            storage: T::AccountId,
            cid: Vec<u8>,
            pool_id: u16,
        },
        RemoveStorerOutput {
            uploader: T::AccountId,
            storage: Option<T::AccountId>,
            cid: Vec<u8>,
            pool_id: u16,
        },
        ManifestRemoved {
            uploader: T::AccountId,
            cid: Vec<u8>,
            pool_id: u16,
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
        ManifestAlreadyExist,
        ManifestNotFound,
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
            storage: T::AccountId,
            manifest: ManifestMetadataOf<T>,
            cid: ManifestCIDOf<T>,
            pool_id: u16,
            replication_factor: u16,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            Self::do_update_manifest(&who, &storage, manifest, cid, pool_id, replication_factor)?;
            Ok(().into())
        }

        #[pallet::weight(10_000)]
        pub fn upload_manifest(
            origin: OriginFor<T>,
            manifest: ManifestMetadataOf<T>,
            cid: ManifestCIDOf<T>,
            pool_id: u16,
            replication_factor: u16,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            Self::do_upload_manifest(&who, manifest, cid, pool_id, replication_factor)?;
            Ok(().into())
        }

        #[pallet::weight(10_000)]
        pub fn storage_manifest(
            origin: OriginFor<T>,
            uploader: T::AccountId,
            cid: ManifestCIDOf<T>,
            pool_id: u16,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            Self::do_storage_manifest(&who, &uploader, cid, pool_id)?;
            Ok(().into())
        }

        #[pallet::weight(10_000)]
        pub fn remove_storer(
            origin: OriginFor<T>,
            storage: T::AccountId,
            cid: ManifestCIDOf<T>,
            pool_id: u16,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            Self::do_remove_storer(&storage, &who, cid, pool_id)?;
            Ok(().into())
        }

        #[pallet::weight(10_000)]
        pub fn remove_stored_manifest(
            origin: OriginFor<T>,
            uploader: T::AccountId,
            cid: ManifestCIDOf<T>,
            pool_id: u16,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            Self::do_remove_storer(&who, &uploader, cid, pool_id)?;
            Ok(().into())
        }

        #[pallet::weight(10_000)]
        pub fn remove_manifest(
            origin: OriginFor<T>,
            cid: ManifestCIDOf<T>,
            pool_id: u16,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            Self::do_remove_manifest(&who, cid, pool_id)?;
            Ok(().into())
        }
    }
}

impl<T: Config> Pallet<T> {

    pub fn do_upload_manifest(
        uploader: &T::AccountId,
        manifest: ManifestMetadataOf<T>,
        cid: ManifestCIDOf<T>,
        pool_id: u16,
        replication_factor: u16,
    ) -> DispatchResult {
        ensure!(
            Manifests::<T>::try_get((pool_id, uploader.clone(), CID(cid.clone()))).is_err(), 
            Error::<T>::ManifestAlreadyExist
        );
        ensure!(
            replication_factor > 0,
            Error::<T>::ReplicationFactorInvalid
        );
        let empty = Vec::<T::AccountId>::new();
        Manifests::<T>::insert(
            (
                pool_id, 
                uploader,
                CID(cid)
            ),
            Manifest{
                storage: empty.to_owned(),
                replication_factor,
                manifest_data: ManifestData {
                    uploader: uploader.clone(),
                    manifest_metadata: manifest.clone(),
                },
            }        
        );

        Self::deposit_event(Event::ManifestOutput {
            uploader: uploader.clone(),
            storage: empty.to_owned(),
            manifest: manifest.to_vec(),
            pool_id: pool_id.clone(),
        });
        Ok(())
    }

    pub fn do_update_manifest(
        uploader: &T::AccountId,
        storage: &T::AccountId,
        manifest: ManifestMetadataOf<T>,
        cid: ManifestCIDOf<T>,
        pool_id: u16,
        replication_factor: u16,
    ) -> DispatchResult {
        ensure!(
            Manifests::<T>::try_get((pool_id, uploader.clone(), CID(cid.clone()))).is_ok(), 
            Error::<T>::ManifestNotFound
        );
        ensure!(
            replication_factor > 0,
            Error::<T>::ReplicationFactorInvalid
        );
        let mut storer_vec = Vec::<T::AccountId>::new();
        storer_vec.push(storage.clone());

        Manifests::<T>::insert(
            (
                pool_id,
                uploader,
                CID(cid)
            ),
            Manifest{
                storage: storer_vec.to_owned(),
                replication_factor,
                manifest_data: ManifestData {
                    uploader: uploader.clone(),
                    manifest_metadata: manifest.clone(),
                },
            }        
        );

        Self::deposit_event(Event::ManifestOutput {
            uploader: uploader.clone(),
            storage: storer_vec.to_owned(),
            manifest: manifest.to_vec(),
            pool_id: pool_id.clone(),
        });
        Ok(()) 
    }

    pub fn do_storage_manifest(
        storage: &T::AccountId,
        uploader: &T::AccountId,
        cid: ManifestCIDOf<T>,
        pool_id: u16,
    ) -> DispatchResult {
        ensure!(
            Manifests::<T>::try_get((pool_id, uploader.clone(), CID(cid.clone()))).is_ok(), 
            Error::<T>::ManifestNotFound
        );
        Manifests::<T>::try_mutate(
            (
                pool_id, 
                uploader,
                CID(cid.clone())
            ),
        |value| -> DispatchResult {
            if let Some(manifest) = value {
                ensure!(
                    manifest.storage.len() < manifest.replication_factor.into(), 
                    Error::<T>::ReplicationFactorLimitReached
                );
                ensure!(
                    !manifest.storage.contains(storage), 
                    Error::<T>::AccountAlreadyStorer
                );
                manifest.storage.push(storage.clone());
            }
            Ok(())
            }       
        )?;

        Self::deposit_event(Event::StorageManifestOutput {
            uploader: uploader.clone(),
            storage: storage.clone(),
            cid: cid.to_vec(),
            pool_id: pool_id.clone(),
        });
        Ok(())
    }

    pub fn do_remove_manifest(
        uploader: &T::AccountId,
        cid: ManifestCIDOf<T>,
        pool_id: u16,
    ) -> DispatchResult {
        ensure!(
            Manifests::<T>::try_get((pool_id, uploader.clone(), CID(cid.clone()))).is_ok(), 
            Error::<T>::ManifestNotFound
        );
        Manifests::<T>::remove((pool_id, uploader, CID(cid.clone())));

        Self::deposit_event(Event::ManifestRemoved {
            uploader: uploader.clone(),
            cid: cid.to_vec(),
            pool_id: pool_id.clone(),
        });
        Ok(())
    }

    pub fn do_remove_storer(
        storage: &T::AccountId,
        uploader: &T::AccountId,
        cid: ManifestCIDOf<T>,
        pool_id: u16,
    ) -> DispatchResult {
        ensure!(
            Manifests::<T>::try_get((pool_id, uploader.clone(), CID(cid.clone()))).is_ok(), 
            Error::<T>::ManifestNotFound
        );
        let mut removed_storer = None;
        Manifests::<T>::try_mutate(
            (
                pool_id, 
                uploader,
                CID(cid.clone())
            ),
            |value| -> DispatchResult {
                if let Some(manifest) = value {
                    ensure!(
                        manifest.storage.contains(storage), 
                        Error::<T>::AccountNotStorer
                    );
                    let value_removed = manifest.storage.swap_remove(manifest.storage.iter().position(|x| { *x == storage.clone()}).unwrap());
                    removed_storer = Some(value_removed);
                    
                }
                Ok(())
            }       
        )?;
    
        Self::deposit_event(Event::RemoveStorerOutput {
            uploader: uploader.clone(),
            storage: removed_storer,
            cid: cid.to_vec(),
            pool_id: pool_id.clone(),
        });
        Ok(())
    }
}
