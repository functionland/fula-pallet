#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{dispatch::DispatchResult, traits::Get, BoundedVec};
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
    pub from: AccountId,
    pub to: AccountId,
    pub manifest: ManifestMetadataOf,
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
        type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

        #[pallet::constant]
        type MaxManifestMetadata: Get<u32>;
    }

    pub type ManifestMetadataOf<T> = BoundedVec<u8, <T as Config>::MaxManifestMetadata>;
    pub type ManifestOf<T> =
        Manifest<<T as frame_system::Config>::AccountId, ManifestMetadataOf<T>>;

    #[pallet::pallet]
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
            NMapKey<Blake2_128Concat, T::AccountId>,
            NMapKey<Blake2_128Concat, T::AccountId>,
            NMapKey<Blake2_128Concat, ManifestOf<T>>,
        ),
        ManifestOf<T>
    >;

    // Pallets use events to inform users when important changes are made.
    // https://docs.substrate.io/v3/runtime/events-and-errors
    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        ManifestUpdated {
            from: T::AccountId,
            to: T::AccountId,
            manifest: Vec<u8>,
        },
    }

    // Errors inform users that something went wrong.
    #[pallet::error]
    pub enum Error<T> {
        /// Error names should be descriptive.
        NoneValue,
        /// Errors should have helpful documentation associated with them.
        StorageOverflow,
    }

    // Dispatchable functions allows users to interact with the pallet and invoke state changes.
    // These functions materialize as "extrinsics", which are often compared to transactions.
    // Dispatchable functions must be annotated with a weight and must return a DispatchResult.
    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Updates fula manifest from to
        #[pallet::weight(10_000)]
        pub fn update_manifest(
            origin: OriginFor<T>,
            to: T::AccountId,
            manifest: ManifestMetadataOf<T>,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            Self::do_update_manifest(&who, &to, manifest)?;
            Ok(().into())
        }
    }
}

impl<T: Config> Pallet<T> {
    pub fn do_update_manifest(
        from: &T::AccountId,
        to: &T::AccountId,
        manifest: ManifestMetadataOf<T>,
    ) -> DispatchResult {
        Manifests::<T>::insert(
            (to,
            from,
            &Manifest {
                from: from.clone(),
                to: to.clone(),
                manifest: manifest.clone(),
            }),
            &Manifest {
                from: from.clone(),
                to: to.clone(),
                manifest: manifest.clone(),
            }
        );

        Self::deposit_event(Event::ManifestUpdated {
            from: from.clone(),
            to: to.clone(),
            manifest: manifest.to_vec(),
        });

        Ok(())
    }
}
