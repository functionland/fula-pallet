#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{dispatch::DispatchResult, ensure, traits::Get, BoundedVec};
use frame_system::{self as system};
// SBP-M1 review: consider refactoring into a separate crate within fula_pool repo to lighten dependencies
use fula_pool::PoolInterface;
use libm::exp;
use scale_info::TypeInfo;
use sp_runtime::{traits::{BlakeTwo256, Hash}, RuntimeDebug};
use sp_std::{prelude::*, vec::Vec};

pub use pallet::*;

// SBP-M1 review: group constants in block (formatting)
// SBP-M1 review: number literal lacking separators
const YEARLY_TOKENS: u64 = 48000000;
// SBP-M1 review: floating point usage may not be deterministic, using sp_arithmetic::per_things is preferred
// SBP-M1 review: potential loss of precision in casting
const DAILY_TOKENS_MINING: f64 = YEARLY_TOKENS as f64 * 0.70 / (12 * 30) as f64;
const DAILY_TOKENS_STORAGE: f64 = YEARLY_TOKENS as f64 * 0.20 / (12 * 30) as f64;

const NUMBER_CYCLES_TO_ADVANCE: u16 = 3;
const NUMBER_CYCLES_TO_RESET: u16 = 3;

// SBP-M1 review: missing trait doc comments
pub trait MaxRange {
    type Range;
    fn random(max_range: Self::Range) -> u64;
}

pub type Range = u64;

#[cfg(test)]
mod mock;

// SBP-M1 review: insufficient test coverage, no test for any dispatchable function
#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

// SBP-M1 review: consider moving structs to separate module

/// Manifest struct store the data related to an specific CID
#[derive(Encode, Decode, Clone, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct Manifest<AccountId, ManifestMetadataOf> {
    // SBP-M1 review: use BoundedVec to ensure state changes remain within block limits - see weights/benchmarking
    // SBP-M1 review: consider moving to its own storage map, keyed by manifest, user etc. May not be scalable as users_data grows
    pub users_data: Vec<UploaderData<AccountId>>,
    // SBP-M1 review: consider changing type to BoundedVec<u8, MaxManifestMetadata> where MaxManifestMetadata : Get<u32> so less opaque
    pub manifest_metadata: ManifestMetadataOf,
    pub size: Option<FileSize>,
}

/// Struct to store the data related to the uploader of a manifest
#[derive(Encode, Decode, Clone, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct UploaderData<AccountId> {
    pub uploader: AccountId,
    // SBP-M1 review: use BoundedVec to ensure state changes remain within block limits - see weights/benchmarking
    pub storers: Vec<AccountId>,
    pub replication_factor: ReplicationFactor,
}

/// Manifest struct for the call Get_manifests
#[derive(Encode, Decode, Clone, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
// SBP-M1 review: 'getter' structs, move to runtime api
pub struct ManifestWithPoolId<PoolId, AccountId, ManifestMetadataOf> {
    pub pool_id: PoolId,
    // SBP-M1 review: use BoundedVec to ensure state changes remain within block limits - see weights/benchmarking
    pub users_data: Vec<UploaderData<AccountId>>,
    pub manifest_metadata: ManifestMetadataOf,
    pub size: Option<FileSize>,
}

/// Manifest struct for the call Get_available_manifests
#[derive(Encode, Decode, Clone, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
// SBP-M1 review: 'getter' structs, move to runtime api
pub struct ManifestAvailable<PoolId, ManifestMetadataOf> {
    pub pool_id: PoolId,
    pub replication_factor: ReplicationFactor,
    pub manifest_metadata: ManifestMetadataOf,
}

/// Manifest struct for the call Get_manifest_storers_data
#[derive(Encode, Decode, Clone, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct StorerData<PoolId, Cid, AccountId> {
    pub pool_id: PoolId,
    pub cid: Cid,
    pub account: AccountId,
    pub manifest_data: ManifestStorageData,
}

/// Challenge Struct that will store the Open challenges to verify the existence of a file in the challenged IPFS node
#[derive(Encode, Decode, Clone, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct Challenge<AccountId> {
    pub challenger: AccountId,
    pub challenge_state: ChallengeState,
}

/// Enum that represent the current verify state of a file on-chain if a challenge was successful / failed / open
#[derive(Encode, Decode, Clone, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum ChallengeState {
    Open,
    Successful,
    Failed,
}

/// Manifests struct to store the variables needed to the calculation of the labor_tokens of a given file
#[derive(Encode, Decode, Clone, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct ManifestStorageData {
    pub active_cycles: Cycles,
    pub missed_cycles: Cycles,
    pub active_days: ActiveDays,
    pub challenge_state: ChallengeState,
}

/// Struct to store the values of the claims made by users
#[derive(Encode, Decode, Clone, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct ClaimData {
    pub minted_labor_tokens: MintBalance,
    pub expected_labor_tokens: MintBalance,
    pub challenge_tokens: MintBalance,
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{dispatch::DispatchResultWithPostInfo, pallet_prelude::*};
    use frame_system::pallet_prelude::*;

    #[pallet::config]
    // SBP-M1 review: use loose pallet coupling (https://docs.substrate.io/reference/how-to-guides/pallet-design/use-loose-coupling/)
    // SBP-M1 review: if no trait is available, define a trait within this pallet and then have the runtime implement the trait, calling into the function of the other pallet (Asset::do_mint)
    pub trait Config: frame_system::Config + sugarfunge_asset::Config {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// Constant values
        #[pallet::constant]
        type MaxManifestMetadata: Get<u32>;
        type MaxCID: Get<u32>;
        type Pool: PoolInterface<AccountId = Self::AccountId>;
    }
    // SBP-M1 review: type aliases probably dont all need to be pub, remove pub modifiier or use pub(super), pub(crate) as required
    // Custom types used to handle the calls and events
    pub type ManifestMetadataOf<T> = BoundedVec<u8, <T as Config>::MaxManifestMetadata>;
    pub type CIDOf<T> = BoundedVec<u8, <T as Config>::MaxCID>;
    pub type PoolIdOf<T> = <<T as Config>::Pool as PoolInterface>::PoolId;
    // SBP-M1 review: consider moving these types to the pallet config trait so they can be defined by chain runtime
    pub type ClassId = u64;
    pub type AssetId = u64;
    pub type MintBalance = u128;
    pub type FileSize = u64;
    pub type ReplicationFactor = u16;
    pub type Cycles = u16;
    // SBP-M1 review: should active days not be an unsigned integer? All usages appear to only increment
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
    // SBP-M1 review: remove bounded storage escape in readiness for future parachain
    #[pallet::without_storage_info]

    pub struct Pallet<T>(_);

    /// Storage to keep the manifest data - Keys: Pool_id (Pool of the file) - CID (Identifier of the file)
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

    /// Storage to keep the data of the storers - Pool_id (Pool of the file) - Account (of the storer) - CID (Identifier of the file)
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
        // SBP-M1 review: OptionQuery is default so can be removed
        OptionQuery,
    >;

    /// Storage to keep the open challenge Requests to verify the existence of a file - Keys: Account (of the user challenged) - CID (Identifier of the file)
    #[pallet::storage]
    #[pallet::getter(fn challenges)]
    pub(super) type ChallengeRequests<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        // SBP-M1 review: consider AccountIdOf<T> type alias for consistency
        T::AccountId,
        Blake2_128Concat,
        CIDOf<T>,
        ChallengeRequestsOf<T>,
    >;

    /// Storage to keep the claim data of the users
    #[pallet::storage]
    #[pallet::getter(fn claims)]
    pub(super) type Claims<T: Config> =
        // SBP-M1 review: OptionQuery is default so can be removed
        StorageMap<_, Blake2_128Concat, T::AccountId, ClaimData, OptionQuery>;

    /// A value to keep track of the Network size when a file is stored or removed
    #[pallet::storage]
    // SBP-M1 review: restrict visibility > pub(super)
    pub type NetworkSize<T: Config> = StorageValue<_, u64, ValueQuery>;

    // SBP-M1 review: consider adding doc comments for enum variants (and fields)
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
        UpdateFileSizeOutput {
            account: T::AccountId,
            pool_id: PoolIdOf<T>,
            cid: Vec<u8>,
            size: u64,
        },
        UpdateFileSizesOutput {
            account: T::AccountId,
            pool_id: PoolIdOf<T>,
            cids: Vec<Vec<u8>>,
            sizes: Vec<u64>,
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
        VerifiedChallenges {
            challenged: T::AccountId,
            successful: Vec<Vec<u8>>,
            failed: Vec<Vec<u8>>,
        },
        MintedLaborTokens {
            account: T::AccountId,
            class_id: ClassId,
            asset_id: AssetId,
            amount: MintBalance,
            calculated_amount: MintBalance,
        },
    }

    /// Errors inform users that something went wrong.
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
        AccountNotFound,
        ManifestAlreadyExist,
        ManifestNotFound,
        ManifestNotStored,
        InvalidArrayLength,
        ErrorPickingCIDToChallenge,
        ErrorPickingAccountToChallenge,
        ManifestStorerDataNotFound,
        NoFileSizeProvided,
        NoAccountsToChallenge,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        // Updates the values of the manifest storer data given the specific data
        #[pallet::call_index(0)]
        // SBP-M1 review: implement benchmark and use resulting weight function
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        pub fn update_manifest(
            origin: OriginFor<T>,
            cid: CIDOf<T>,
            pool_id: PoolIdOf<T>,
            active_cycles: Cycles,
            missed_cycles: Cycles,
            active_days: ActiveDays,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_update_manifest(
                who,
                pool_id,
                &cid,
                active_days,
                active_cycles,
                missed_cycles,
            )
        }

        // Upload a manifest to the chain
        #[pallet::call_index(1)]
        // SBP-M1 review: implement benchmark and use resulting weight function
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        pub fn upload_manifest(
            origin: OriginFor<T>,
            manifest: ManifestMetadataOf<T>,
            cid: CIDOf<T>,
            pool_id: PoolIdOf<T>,
            replication_factor: ReplicationFactor,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_upload_manifest(&who, pool_id, &cid, manifest, replication_factor)
        }

        // Upload multiple manifests to the chain
        #[pallet::call_index(2)]
        // SBP-M1 review: implement benchmark (with complexity parameter) and use resulting weight function
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        pub fn batch_upload_manifest(
            origin: OriginFor<T>,
            // SBP-M1 review: use BoundedVec for all Vec parameters, sized appropriately based on weight consumption (benchmarks)
            // SBP-M1 review: a vector of tuples would make more sense: BoundedVec<(CIDOf<T>, PoolIdOf<T>, ReplicationFactor,...)>
            manifest: Vec<ManifestMetadataOf<T>>,
            cids: Vec<CIDOf<T>>,
            pool_id: Vec<PoolIdOf<T>>,
            replication_factor: Vec<ReplicationFactor>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_batch_upload_manifest(who, pool_id, &cids, manifest, &replication_factor)
        }

        // SBP-M1 review: improve description
        // Storage an available manifest from the chain
        #[pallet::call_index(3)]
        // SBP-M1 review: implement benchmark and use resulting weight function
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        // SBP-M1 review: consider clearer name for dispatchable function - e.g. add_storer_to_manifest
        pub fn storage_manifest(
            origin: OriginFor<T>,
            cid: CIDOf<T>,
            pool_id: PoolIdOf<T>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_storage_manifest(&who, pool_id, &cid)
        }

        // Storage multiple manifests from the chain
        #[pallet::call_index(4)]
        // SBP-M1 review: implement benchmark and use resulting weight function
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        // SBP-M1 review: consider clearer name aligned with storage_manifest(..) suggestion
        pub fn batch_storage_manifest(
            origin: OriginFor<T>,
            // SBP-M1 review: use BoundedVec for all Vec parameters, sized appropriately based on weight consumption (benchmarks)
            cids: Vec<CIDOf<T>>,
            pool_id: PoolIdOf<T>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_batch_storage_manifest(who, pool_id, cids)
        }

        // A storer remove a manifest that was being stored on chain
        #[pallet::call_index(5)]
        // SBP-M1 review: implement benchmark and use resulting weight function
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        // SBP-M1 review: consider renaming - the manifest isn't removed, only the ManifestsStorerData. Perhaps remove_storer_from_manifest?
        pub fn remove_stored_manifest(
            origin: OriginFor<T>,
            cid: CIDOf<T>,
            pool_id: PoolIdOf<T>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_remove_storer(&who, pool_id, &cid)
        }

        // A storer removed multiple manifests that were being stored on chain
        #[pallet::call_index(6)]
        // SBP-M1 review: implement benchmark and use resulting weight function
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        // SBP-M1 review: consider renaming to align with above
        pub fn batch_remove_stored_manifest(
            origin: OriginFor<T>,
            // SBP-M1 review: use BoundedVec for all Vec parameters, sized appropriately based on weight consumption (benchmarks)
            cids: Vec<CIDOf<T>>,
            pool_id: PoolIdOf<T>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_batch_remove_storer(who, pool_id, cids)
        }

        // The Uploader remove the manifest from the chain
        #[pallet::call_index(7)]
        // SBP-M1 review: implement benchmark and use resulting weight function
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        pub fn remove_manifest(
            origin: OriginFor<T>,
            cid: CIDOf<T>,
            pool_id: PoolIdOf<T>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_remove_manifest(&who, pool_id, &cid)
        }

        // The Uploader remove multiple manifests from the chain
        #[pallet::call_index(8)]
        // SBP-M1 review: implement benchmark and use resulting weight function
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        pub fn batch_remove_manifest(
            origin: OriginFor<T>,
            // SBP-M1 review: use BoundedVec for all Vec parameters, sized appropriately based on weight consumption (benchmarks)
            // SBP-M1 review: a vector of tuples would make more sense: BoundedVec<(CIDOf<T>, PoolIdOf<T>)>
            cids: Vec<CIDOf<T>>,
            pool_ids: Vec<PoolIdOf<T>>,
            // SBP-M1 review: can just be DispatchResult as no additional information returned
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_batch_remove_manifest(who, &pool_ids, cids)
        }

        // SBP-M1 review: 'removes them'
        // A storer verifies if there are invalid manifests still stored on chain and removed them
        #[pallet::call_index(9)]
        // SBP-M1 review: implement benchmark and use resulting weight function
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        // SBP-M1 review: why would a storer bother calling this to clean up? It would cost them in tx fees so disincentivized to clean up leading to state bloat. A suggestion is to take a deposit for storage, so they are incentivized to clean up to reclaim deposit
        // SBP-M1 review: add optional parameter to specify the maximum number of ManifestStorageData items to check per call
        pub fn verify_manifests(origin: OriginFor<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_verify_manifests(who)
        }

        // SBP-M1 review: assume this dispatchable is acting as a 'getter', which is not the intended usage of dispatchable functions
        // SBP-M1 review: a runtime-api is probably what you are after (https://docs.substrate.io/reference/runtime-apis/), which can be called via the state_call RPC endpoint
        // A call to get all the manifest from the chain with possible filters
        #[pallet::call_index(10)]
        // SBP-M1 review: implement benchmark and use resulting weight function
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        pub fn get_manifests(
            _origin: OriginFor<T>,
            pool_id: Option<PoolIdOf<T>>,
            uploader: Option<T::AccountId>,
            storer: Option<T::AccountId>,
        ) -> DispatchResultWithPostInfo {
            // SBP-M1 review: origin check expected
            Self::do_get_manifests(pool_id, uploader, storer)?;
            Ok(().into())
        }

        // SBP-M1 review: assume this dispatchable is acting as a 'getter', which is not the intended usage of dispatchable functions
        // SBP-M1 review: a runtime-api is probably what you are after (https://docs.substrate.io/reference/runtime-apis/), which can be called via the state_call RPC endpoint
        // A call to get all available manifests from the chain with possible filters
        #[pallet::call_index(11)]
        // SBP-M1 review: implement benchmark and use resulting weight function
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        pub fn get_available_manifests(
            _origin: OriginFor<T>,
            pool_id: Option<PoolIdOf<T>>,
        ) -> DispatchResultWithPostInfo {
            // SBP-M1 review: origin check expected
            Self::do_get_available_manifests(pool_id)?;
            Ok(().into())
        }

        // SBP-M1 review: assume this dispatchable is acting as a 'getter', which is not the intended usage of dispatchable functions
        // SBP-M1 review: a runtime-api is probably what you are after (https://docs.substrate.io/reference/runtime-apis/), which can be called via the state_call RPC endpoint
        // A call to get all the storers data from the chain with possible filters
        #[pallet::call_index(12)]
        // SBP-M1 review: implement benchmark and use resulting weight function
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        pub fn get_manifests_storer_data(
            _origin: OriginFor<T>,
            pool_id: Option<PoolIdOf<T>>,
            storer: Option<T::AccountId>,
        ) -> DispatchResultWithPostInfo {
            // SBP-M1 review: origin check expected
            Self::do_get_manifest_storer_data(pool_id, storer)?;
            Ok(().into())
        }

        // Generates a challenge to verify if a storer is still holding a random CID that he has on chain
        #[pallet::call_index(13)]
        // SBP-M1 review: implement benchmark and use resulting weight function
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        pub fn generate_challenge(origin: OriginFor<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_generate_challenge(who)
        }

        // Verifies if the challenged account has the CID that was challenged stored
        #[pallet::call_index(14)]
        // SBP-M1 review: implement benchmark and use resulting weight function
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        // SBP-M1 review: can the challenged not simply call this function with any cid defined in public challenge request state (or emitted event) as parameter to result in a successful challenge?
        pub fn verify_challenge(
            origin: OriginFor<T>,
            pool_id: PoolIdOf<T>,
            // SBP-M1 review: use BoundedVec for all Vec parameters, sized appropriately based on weight consumption (benchmarks)
            cids: Vec<CIDOf<T>>,
            // SBP-M1 review: no verification of these parameters to ensure they pertain to 'challenge tokens'. Seems caller could specify anything and have them minted, depending on assets defined within assets pallet. Perhaps some allowlist configured on the pallet?
            class_id: ClassId,
            asset_id: AssetId,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            // SBP-M1 review: take ownership of challenged rather than reference as `who` is not used again
            Self::do_verify_challenge(&who, &cids, pool_id, class_id, asset_id)
        }

        // Mint the labor tokens that are going to be used to changed for claimed tokens and then for Fula Tokens - This correspond to the previously known rewards
        #[pallet::call_index(15)]
        // SBP-M1 review: implement benchmark and use resulting weight function
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        pub fn mint_labor_tokens(
            origin: OriginFor<T>,
            // SBP-M1 review: no verification of these parameters to ensure they pertain to 'labour tokens'. Seems caller could specify anything and have them minted, depending on assets defined within assets pallet. Perhaps some allowlist configured on the pallet?
            class_id: ClassId,
            asset_id: AssetId,
            amount: MintBalance,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_mint_labor_tokens(who, class_id, asset_id, amount)
        }

        // Updates the file_size of a manifest in the chain
        #[pallet::call_index(16)]
        // SBP-M1 review: implement benchmark and use resulting weight function
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        pub fn update_file_size(
            origin: OriginFor<T>,
            cid: CIDOf<T>,
            pool_id: PoolIdOf<T>,
            // SBP-M1 review: no controls to ensure that size is valid. Caller can just specify max size to maximise rewards
            size: FileSize,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_update_size(&who, pool_id, &cid, size)
        }

        // Updates multiple file_sizes of manifests in the chain
        #[pallet::call_index(17)]
        // SBP-M1 review: implement benchmark and use resulting weight function
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        pub fn update_file_sizes(
            origin: OriginFor<T>,
            // SBP-M1 review: use BoundedVec for all Vec parameters, sized appropriately based on weight consumption (benchmarks)
            // SBP-M1 review: a vector of tuples would make more sense: BoundedVec<(CIDOf<T>, FileSize)>
            cids: Vec<CIDOf<T>>,
            pool_id: PoolIdOf<T>,
            sizes: Vec<FileSize>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_update_sizes(who, pool_id, cids, sizes)
        }
    }

    impl<T: Config> MaxRange for Pallet<T> {
        type Range = Range;
        // SBP-M1 review: consider using the Randomness trait (https://docs.substrate.io/build/randomness/) as an improvement, provided the randomness source available via the node runtime is sufficiently secure for your needs
        // SBP-M1 review: https://github.com/paritytech/substrate/blob/master/frame/lottery/src/lib.rs may also be helpful
        fn random(max_range: Self::Range) -> u64 {
            let block_number = <system::Pallet<T>>::block_number();

            // SBP-M1 review: 'let input: Vec<u8> = block_number.encode()'
            let mut input = Vec::new();

            input.extend_from_slice(&block_number.encode());

            let hash_result = BlakeTwo256::hash_of(&input);

            let random_number = T::Hashing::hash(hash_result.as_bytes());

            // SBP-M1 review: use safe math
            // SBP-M1 review: possible truncation with casting
            let result = random_number
                .as_ref()
                .iter()
                .take(8) // take only the first 8 bytes
                .fold(0, |acc, &byte| (acc << 8) + byte as u32)
                % max_range as u32;

            // SBP-M1 review: unneeded return statement > use cargo clippy
            return result as u64;
        }
    }
}

// SBP-M1 review: consider moving impl functions to separate module
impl<T: Config> Pallet<T> {
    // SBP-M1 review: should probably be pub(crate) at most
    // SBP-M1 review: function too long, needs refactoring
    pub fn do_upload_manifest(
        uploader: &T::AccountId,
        pool_id: PoolIdOf<T>,
        cid: &CIDOf<T>,
        manifest: ManifestMetadataOf<T>,
        replication_factor: ReplicationFactor,
    ) -> DispatchResult {
        // Validations made to verify some parameters given
        ensure!(replication_factor > 0, Error::<T>::ReplicationFactorInvalid);

        // SBP-M1 review: typo > 'auxiliary'?
        // Auxiliar structures
        // SBP-M1 review: move definition to inner scope where used, otherwise unnecessary creation if manifest exists
        let mut uploader_vec = Vec::new();
        // SBP-M1 review: move storers_vec to UploaderData.storers init
        let storers_vec = Vec::new();

        let uploader_data = UploaderData {
            uploader: uploader.clone(),
            storers: storers_vec.to_owned(),
            replication_factor,
        };
        // SBP-M1 review: fix comment, appears something was removed which no longer makes sense
        // If the CID of the manifest already exists the  it creates another Uploader into the Vec of uploader data, if not then it creates the CID in the manifests storage
        // SBP-M1 review: wasteful .clone(), & can be used with EncodeLike key
        // SBP-M1 review: switch to <Manifests<T>>::mutate(..) with inner match on value to 'upsert'
        if let Some(_manifest) = Self::manifests(pool_id, cid.clone()) {
            // SBP-M1 review: wasteful .clone(), & can be used with EncodeLike key
            // SBP-M1 review: try_mutate not required as no error returned, use mutate
            Manifests::<T>::try_mutate(pool_id, &cid, |value| -> DispatchResult {
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
                    // SBP-M1 review: could be simplified with users_data: vec![uploader_data]
                    users_data: uploader_vec.to_owned(),
                    manifest_metadata: manifest.clone(),
                    size: None,
                },
            );
        }

        // SBP-M1 review: wasteful clones and to_vec() > use cargo clippy
        Self::deposit_event(Event::ManifestOutput {
            uploader: uploader.clone(),
            // SBP-M1 review: storers_vec always empty
            storer: storers_vec.to_owned(),
            manifest: manifest.to_vec(),
            pool_id: pool_id.clone(),
        });
        Ok(())
    }

    // SBP-M1 review: should probably be pub(crate) at most
    pub fn do_batch_upload_manifest(
        uploader: T::AccountId,
        pool_ids: Vec<PoolIdOf<T>>,
        cids: &Vec<CIDOf<T>>,
        manifests: Vec<ManifestMetadataOf<T>>,
        replication_factors: &Vec<ReplicationFactor>,
    ) -> DispatchResult {
        // Validations made to verify some parameters given
        ensure!(
            cids.len() == manifests.len(),
            Error::<T>::InvalidArrayLength
        );
        ensure!(
            cids.len() == replication_factors.len(),
            Error::<T>::InvalidArrayLength
        );

        // The cycle to execute multiple times the upload_manifest call
        // SBP-M1 review: loop should be bounded to not exceed block limits (BoundedVec and benchmarking)
        for (i, cid) in cids.into_iter().enumerate() {
            // SBP-M1 review: indexing may panic
            let manifest = manifests[i].to_owned();
            let replication_factor = replication_factors[i];
            let pool_id = pool_ids[i];
            Self::do_upload_manifest(&uploader, pool_id, cid, manifest, replication_factor)?;
        }

        // SBP-M1 review: wasteful clones > use cargo clippy
        Self::deposit_event(Event::BatchManifestOutput {
            uploader: uploader.clone(),
            manifests: Self::transform_manifest_to_vec(manifests),
            pool_ids: pool_ids.clone(),
        });
        Ok(())
    }

    // SBP-M1 review: should probably be pub(crate) at most
    pub fn do_update_manifest(
        storer: T::AccountId,
        pool_id: PoolIdOf<T>,
        cid: &CIDOf<T>,
        active_days: ActiveDays,
        active_cycles: Cycles,
        missed_cycles: Cycles,
    ) -> DispatchResult {
        // Validations made to verify some parameters given
        ensure!(
            // SBP-M1 review: update trait definition to allow borrow to determine membership to avoid unnecessary clone
            T::Pool::is_member(storer.clone(), pool_id),
            Error::<T>::AccountNotInPool
        );
        ensure!(
            // SBP-M1 review: use .contains_key() as cheaper and pass keys by reference instead of wasteful cloning
            ManifestsStorerData::<T>::try_get((pool_id, storer.clone(), cid.clone())).is_ok(),
            Error::<T>::ManifestNotStored
        );

        // Update the values in the ManifestStorerData storage
        ManifestsStorerData::<T>::mutate(
            (pool_id, &storer, &cid),
            |value| -> DispatchResult {
                if let Some(manifest) = value {
                    manifest.active_cycles = active_cycles;
                    manifest.missed_cycles = missed_cycles;
                    manifest.active_days = active_days;
                }
                Ok(())
            },
        )?;

        // SBP-M1 review: wasteful to_vec() > use cargo clippy
        Self::deposit_event(Event::ManifestStorageUpdated {
            storer,
            pool_id,
            cid: cid.to_vec(),
            active_cycles,
            missed_cycles,
            active_days,
        });
        Ok(())
    }

    // SBP-M1 review: should probably be pub(crate) at most
    // SBP-M1 review: function too long, needs refactoring
    pub fn do_storage_manifest(
        storer: &T::AccountId,
        pool_id: PoolIdOf<T>,
        cid: &CIDOf<T>,
    ) -> DispatchResult {
        // Validations made to verify some parameters given
        ensure!(
            // SBP-M1 review: update trait definition to allow borrow to determine membership to avoid unnecessary clone
            T::Pool::is_member(storer.clone(), pool_id),
            Error::<T>::AccountNotInPool
        );
        ensure!(
            Manifests::<T>::contains_key(pool_id, &cid),
            Error::<T>::ManifestNotFound
        );
        // Add the storer to the manifest storage
        // SBP-M1 review: pass keys by reference instead of wasteful cloning
        Manifests::<T>::try_mutate(pool_id, &cid, |value| -> DispatchResult {
            // SBP-M1 review: use let-else to reduce if nesting
            if let Some(manifest) = value {
                // Get the next uploader that has available storers
                if let Some(index) =
                    Self::get_next_available_uploader_index(&manifest.users_data)
                {
                    ensure!(
                        // SBP-M1 review: use .get() and handle properly rather than []
                        !manifest.users_data[index].storers.contains(&storer),
                        Error::<T>::AccountAlreadyStorer
                    );
                    // SBP-M1 review: indexing may panic
                    manifest.users_data[index].storers.push(storer.clone());
                    // Insert the default data to the storer data storage
                    ManifestsStorerData::<T>::insert(
                        (pool_id, storer, &cid),
                        // SBP-M1 review: could be refactored to ManifestStorageData::new(ChallengeState::Open)
                        ManifestStorageData {
                            active_cycles: 0,
                            missed_cycles: 0,
                            active_days: 0,
                            challenge_state: ChallengeState::Open,
                        },
                    );
                }
                // If the manifest has the file_size available, update the network size
                if let Some(file_size) = manifest.size {
                    NetworkSize::<T>::mutate(|total_value| {
                        // SBP-M1 review: use safe math - e.g. saturating_inc()
                        *total_value += file_size;
                    });
                }
            }
            Ok(())
        })?;

        // SBP-M1 review: wasteful clones and to_vec() > use cargo clippy
        Self::deposit_event(Event::StorageManifestOutput {
            storer: storer.clone(),
            pool_id: pool_id.clone(),
            cid: cid.to_vec(),
        });
        Ok(())
    }

    // SBP-M1 review: should probably be pub(crate) at most
    pub fn do_batch_storage_manifest(
        storer: T::AccountId,
        pool_id: PoolIdOf<T>,
        cids: Vec<CIDOf<T>>,
    ) -> DispatchResult {
        // The cycle to execute multiple times the storage manifests
        for cid in cids.clone() {
            Self::do_storage_manifest(&storer, pool_id, &cid)?;
        }

        // SBP-M1 review: wasteful clones > use cargo clippy
        Self::deposit_event(Event::BatchStorageManifestOutput {
            storer: storer.clone(),
            cids: Self::transform_cid_to_vec(cids),
            pool_id: pool_id.clone(),
        });
        Ok(())
    }

    // SBP-M1 review: should probably be pub(crate) at most
    // SBP-M1 review: function too long, needs refactoring
    pub fn do_remove_manifest(
        uploader: &T::AccountId,
        pool_id: PoolIdOf<T>,
        cid: &CIDOf<T>,
    ) -> DispatchResult {
        // Validations made to verify some parameters given
        ensure!(
            Manifests::<T>::contains_key(pool_id, &cid),
            Error::<T>::ManifestNotFound
        );
        // SBP-M1 review: change to match or let-else/.ok_or()?, returning error as above if no value
        // Get the manifests and the uploaders
        // SBP-M1 review: unwrap may panic
        let manifest = Self::manifests(pool_id, &cid).unwrap();

        // Found the uploader in the vector of uploader data
        // SBP-M1 review: consider let-else to reduce nesting
        if let Some(index) =
            Self::get_uploader_index(&manifest.users_data, &uploader)
        {
            // If there are more uploaders, just remove the value from the vector of uploaders, if not remove the entire manifest
            // SBP-M1 review: wasteful clone > use cargo clippy
            if manifest.users_data.to_owned().len() > 1 {
                Manifests::<T>::mutate(pool_id, &cid, |value| -> DispatchResult {
                    // SBP-M1 review: use let-else to reduce nesting
                    if let Some(manifest) = value {
                        let value_removed = manifest.users_data.remove(index);
                        // SBP-M1 review: double-check comment is accurate, seems to be copied from below
                        // Update the network size, removing the size that belong to the storers before removing the uploader
                        if let Some(file_size) = manifest.size {
                            NetworkSize::<T>::mutate(|total_value| {
                                // SBP-M1 review: use safe math, returning ArithmeticError::Overflow as example
                                *total_value -= file_size * value_removed.storers.len() as u64;
                            });
                        }
                    }
                    Ok(())
                })?;
            } else {
                // Update the network size, removing the size that belong to the storers before removing the uploader
                if let Some(file_size) = manifest.size {
                    NetworkSize::<T>::mutate(|total_value| {
                        // SBP-M1 review: use safe math
                        // SBP-M1 review: use .get() and handle error gracefully rather than [0]
                        // SBP-M1 review: indexing may panic
                        *total_value -= file_size * manifest.users_data[0].storers.len() as u64;
                    });
                }
                Manifests::<T>::remove(pool_id, &cid);
            }
        }

        // SBP-M1 review: event may be emitted despite no state changes above (depending on if blocks)
        // SBP-M1 review: wasteful clones > use cargo clippy
        Self::deposit_event(Event::ManifestRemoved {
            uploader: uploader.clone(),
            cid: cid.to_vec(),
            pool_id: pool_id.clone(),
        });
        Ok(())
    }
    
    // SBP-M1 review: should probably be pub(crate) at most
    pub fn do_batch_remove_manifest(
        uploader: T::AccountId,
        pool_ids: &Vec<PoolIdOf<T>>,
        cids: Vec<CIDOf<T>>,
    ) -> DispatchResult {
        // Validations made to verify some parameters given
        ensure!(cids.len() == pool_ids.len(), Error::<T>::InvalidArrayLength);

        // SBP-M1 review: loop should be bounded to not exceed block limits (BoundedVec and benchmarking)
        // SBP-M1 review: use 'for (i, cid) in cids.into_iter().enumerate() {}' to simplify and avoid cids[i] clone
        // The cycle to execute multiple times the remove manifests
        for (i, cid) in cids.clone().into_iter().enumerate() {
            // SBP-M1 review: indexing may panic
            let pool_id = pool_ids[i];
            Self::do_remove_manifest(&uploader, pool_id, &cid)?;
        }
        
        Self::deposit_event(Event::BatchManifestRemoved {
            uploader,
            cids: Self::transform_cid_to_vec(cids),
            pool_ids: pool_ids.to_vec(),
        });
        Ok(())
    }

    // SBP-M1 review: should probably be pub(crate) at most
    // SBP-M1 review: function too long, needs refactoring
    pub fn do_remove_storer(
        storer: &T::AccountId,
        pool_id: PoolIdOf<T>,
        cid: &CIDOf<T>,
    ) -> DispatchResult {
        // Validations made to verify some parameters given
        ensure!(
            Manifests::<T>::contains_key(pool_id, &cid),
            Error::<T>::ManifestNotFound
        );

        // SBP-M1 review: typo > 'auxiliary'?
        // Auxiliar structures
        // SBP-M1 review:  Manifests::<T>::try_mutate(..) can have a return type, so could simplify with 'let removed_storer = <Manifests<T>>::try_mutate()?'
        // SBP-M1 review:  where the closure return type is changed from DispatchResult to Result<T::AccountId, DispatchError>
        let mut removed_storer = None;

        // Try to remove the storer from the manifest storage
        Manifests::<T>::try_mutate(pool_id, &cid, |value| -> DispatchResult {
            // SBP-M1 review: use let-else to reduce if nesting
            if let Some(manifest) = value {
                // Verify if the account is a storer
                ensure!(
                    Self::verify_account_is_storer(&manifest.users_data, &storer),
                    Error::<T>::AccountNotStorer
                );
                // SBP-M1 review: use let-else to reduce nesting
                // Get the uploader that correspond to the storer
                if let Some(uploader_index) =
                    Self::get_uploader_index_given_storer(&manifest.users_data, &storer)
                {
                    // SBP-M1 review: use let-else to reduce nesting
                    // Get the index of the storer inside the storers vector
                    if let Some(storer_index) = Self::get_storer_index(
                        &manifest.users_data,
                        uploader_index,
                        &storer,
                    ) {
                        // Remove the storer from the storers vector
                        // SBP-M1 review: indexing may panic, use .get() and handle gracefully
                        let value_removed = manifest.users_data[uploader_index]
                            .storers
                            .remove(storer_index);
                        // SBP-M1 review: as mentioned above, could return this value rather than use mutable variable
                        // SBP-M1 review: unnecessary clone
                        removed_storer = Some(value_removed.clone());
                        // Remove the ManifestStorerData
                        // SBP-M1 review: wasteful clones, EncodeLike allows & usage > use cargo clippy
                        ManifestsStorerData::<T>::remove((
                            pool_id,
                            value_removed,
                            cid.clone(),
                        ));
                        // Update the network size, removing the size that belong to the removed storer
                        if let Some(file_size) = manifest.size {
                            NetworkSize::<T>::mutate(|total_value| {
                                // SBP-M1 review: use safe math
                                *total_value -= file_size;
                            });
                        }
                    }
                };
            }
            Ok(())
        })?;

        // SBP-M1 review: wasteful clones/.to_vec() > use cargo clippy
        Self::deposit_event(Event::RemoveStorerOutput {
            storer: removed_storer,
            cid: cid.to_vec(),
            pool_id: pool_id,
        });
        Ok(())
    }

    // SBP-M1 review: should probably be pub(crate) at most
    pub fn do_batch_remove_storer(
        storer: T::AccountId,
        pool_id: PoolIdOf<T>,
        cids: Vec<CIDOf<T>>,
    ) -> DispatchResult {
        // The cycle to execute multiple times the remove storers
        // SBP-M1 review: loops should be bounded to complete within block limits (e.g. BoundedVec)
        // SBP-M1 review: use 'for (i, cid) in cids.into_iter().enumerate() {}' to simplify and avoid cids[i] and clone
        for cid in cids.clone().into_iter() {
            Self::do_remove_storer(&storer, pool_id, &cid)?;
        }

        // SBP-M1 review: wasteful clones > use cargo clippy
        Self::deposit_event(Event::BatchRemoveStorerOutput {
            storer: storer.clone(),
            cids: Self::transform_cid_to_vec(cids),
            pool_id: pool_id.clone(),
        });
        Ok(())
    }

    // SBP-M1 review: should probably be pub(crate) at most
    // SBP-M1 review: function too long, needs refactoring
    pub fn do_verify_manifests(storer: T::AccountId) -> DispatchResult {
        // SBP-M1 review: typo > 'auxiliary'?
        // Auxiliar structures
        let mut invalid_cids = Vec::new();
        let mut valid_cids = Vec::new();

        // SBP-M1 review: loop should be bounded to not exhaust bock limits
        // Verify the valid or invalid cids from the manifest storer data
        // SBP-M1 review: destructure item into clearer variable names
        for item in ManifestsStorerData::<T>::iter() {
            if storer.clone() == item.0 .1 {
                // If the storer has manifests from another pool that is not the current it's invalid and is removed
                // SBP-M1 review: update trait to avoid clone, borrow should be sufficient for checking membership
                // SBP-M1 review: use match statement rather then if-else
                if T::Pool::is_member(item.0 .1.clone(), item.0 .0) {
                    // If the manifest is still in the manifest storage is a valid cid, if not it's invalid and is removed
                    // SBP-M1 review: use match statement rather then if-else
                    if Manifests::<T>::contains_key(item.0 .0, &item.0 .2) {
                        valid_cids.push(item.0 .2.to_vec());
                    } else {
                        invalid_cids.push(item.0 .2.to_vec());

                        ManifestsStorerData::<T>::remove((
                            item.0 .0,
                            &item.0 .1,
                            &item.0 .2,
                        ));
                    }
                } else {
                    invalid_cids.push(item.0 .2.to_vec());
                    ManifestsStorerData::<T>::remove((
                        item.0 .0,
                        &item.0 .1,
                        &item.0 .2,
                    ));
                }
            }
        }

        Self::deposit_event(Event::VerifiedStorerManifests {
            storer,
            valid_cids,
            invalid_cids,
        });
        Ok(())
    }
    
    // SBP-M1 review: should be moved to runtime-api to query chain state
    // SBP-M1 review: should probably be pub(crate) at most
    // SBP-M1 review: function too long, needs refactoring
    pub fn do_get_manifests(
        pool_id: Option<PoolIdOf<T>>,
        // SBP-M1 review: needless pass by value
        uploader: Option<T::AccountId>,
        // SBP-M1 review: needless pass by value
        storer: Option<T::AccountId>,
    ) -> DispatchResult {
        // SBP-M1 review: typo > 'auxiliary'?
        // Auxiliar structures
        let mut manifests_result = Vec::new();

        // Validate the manifests that match the filters
        // SBP-M1 review: destructure item into clearer variable names
        for item in Manifests::<T>::iter() {
            let mut meet_requirements = true;

            // Checks for the pool_id
            if let Some(pool_id_value) = pool_id {
                if pool_id_value != item.0 {
                    meet_requirements = false;
                }
            }

            // Checks for the uploader account
            // SBP-M1 review: can short-circuit if manifest already does not meet requirements
            if let Some(ref uploader_value) = uploader {
                if Self::verify_account_is_uploader(
                    &item.2.users_data,
                    uploader_value,
                ) {
                    meet_requirements = false;
                }
            }

            // Checks for the storer account
            // SBP-M1 review: can short-circuit if manifest already does not meet requirements
            if let Some(ref storer_value) = storer {
                if Self::verify_account_is_storer(&item.2.users_data, &storer_value) {
                    meet_requirements = false;
                }
            }

            // if everything match, add them to the vector of results
            if meet_requirements {
                manifests_result.push(ManifestWithPoolId {
                    pool_id: item.0,
                    users_data: item.2.users_data,
                    manifest_metadata: item.2.manifest_metadata,
                    size: item.2.size,
                });
            }
        }
        // SBP-M1 review: events are technically stored on-chain (temporarily), which is not what you want here
        // SBP-M1 review: remove this and use a runtime-api instead
        Self::deposit_event(Event::GetManifests {
            manifests: manifests_result,
        });
        Ok(())
    }

    // SBP-M1 review: should be moved to runtime-api to query chain state
    // SBP-M1 review: should probably be pub(crate) at most
    pub fn do_get_available_manifests(pool_id: Option<PoolIdOf<T>>) -> DispatchResult {
        // SBP-M1 review: typo > 'auxiliary'?
        // Auxiliar structures
        let mut manifests_result = Vec::new();

        // Validate the manifests that match the filters
        // SBP-M1 review: destructure item into clearer variable names
        for item in Manifests::<T>::iter() {
            // SBP-M1 review: variable not required due to below logic
            let mut meet_requirements = true;

            //Checks that there is still available storers to be added
            // SBP-M1 review: consider .is_some() > use cargo clippy
            // SBP-M1 review: use let-else to reduce nesting, use 'continue'
            if let Some(_) = Self::get_next_available_uploader_index(&item.2.users_data) {
                //Checks for the pool_id
                // SBP-M1 review: use let-else to reduce nesting
                if let Some(pool_id_value) = pool_id {
                    if pool_id_value != item.0 {
                        meet_requirements = false;
                    }
                }

                // if everything match, add them to the vector of results
                // SBP-M1 review: combine with check above, meet_requirements variable not required
                if meet_requirements {
                    manifests_result.push(ManifestAvailable {
                        pool_id: item.0,
                        manifest_metadata: item.2.manifest_metadata,
                        replication_factor: Self::get_added_replication_factor(
                            &item.2.users_data,
                        ),
                    });
                }
            }
        }
        // SBP-M1 review: events are technically stored on-chain (temporarily), which is not what you want here
        // SBP-M1 review: remove this and use a runtime-api instead
        Self::deposit_event(Event::GetAvailableManifests {
            manifests: manifests_result,
        });
        Ok(())
    }

    // SBP-M1 review: should probably be pub(crate) at most
    // SBP-M1 review: function too long, needs refactoring
    pub fn do_get_manifest_storer_data(
        pool_id: Option<PoolIdOf<T>>,
        // SBP-M1 review: needless pass by value
        storer: Option<T::AccountId>,
    ) -> DispatchResult {
        // SBP-M1 review: typo > 'auxiliary'?
        // Auxiliar structures
        let mut manifests_result = Vec::new();

        // Validate the manifests storer data that match the filters
        // SBP-M1 review: destructure item into clearer variable names
        for item in ManifestsStorerData::<T>::iter() {
            let mut meet_requirements = true;

            //Checks for the pool_id
            if let Some(pool_id_value) = pool_id {
                if pool_id_value != item.0 .0 {
                    meet_requirements = false;
                }
            }

            //Checks for the storer account
            // SBP-M1 review: can short-circuit if manifest already does not meet requirements
            if let Some(ref storer_value) = storer {
                if storer_value.clone() != item.0 .1.clone() {
                    meet_requirements = false;
                }
            }

            // if everything match, add them to the vector of results
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

    // SBP-M1 review: should probably be pub(crate) at most
    pub fn pick_random_account_cid_pair() -> (Option<T::AccountId>, Option<CIDOf<T>>) {
        // SBP-M1 review: track this number as a StorageValue rather than iterating over all ManifestsStorerData (which may exhaust block limits) to determine count
        // SBP-M1 review: alternatively use .iter_keys() to avoid returning values which are not used
        let max_value = ManifestsStorerData::<T>::iter().count();
        // SBP-M1 review: instead of generating a random challenge on-chain, perhaps anyone could submit a specific challenge (generated randomly off-chain) and be rewarded if challenged fails
        // SBP-M1 review: use Self instead of pallet::Pallet<T>
        let random_value = <pallet::Pallet<T> as MaxRange>::random(max_value as u64);

        // SBP-M1 review: return can be lifted out of 'if'
        // SBP-M1 review: destructure item into named variables
        // SBP-M1 review: iteration over all state may exhaust block limits and .iter() does not return in any particular order, if that matters
        // SBP-M1 review: it may be more efficient to use .iter_keys() to select the nth key and then query the value to avoid returning all values, of which all but one are discarded
        // SBP-M1 review: consider using iter_key_prefix to limit the scope of the iteration, perhaps by randomly choosing a pool_id first to be used as key prefix, provided that is more efficient
        // SBP-M1 review: cast may truncate
        if let Some(item) = ManifestsStorerData::<T>::iter().nth(random_value as usize) {
            let account = Some(item.0 .1);
            let cid = Some(item.0 .2);
            (account, cid)
        } else {
            (None, None)
        }
    }

    // SBP-M1 review: should probably be pub(crate) at most
    pub fn do_generate_challenge(challenger: T::AccountId) -> DispatchResult {
        let pair = Self::pick_random_account_cid_pair();

        // Validations made to verify some parameters
        // SBP-M1 review: use let x = .ok_or()? to avoid all the unwraps below
        ensure!(pair.0.is_some(), Error::<T>::ErrorPickingAccountToChallenge);
        ensure!(pair.1.is_some(), Error::<T>::ErrorPickingCIDToChallenge);

        // Insert the value in the challenge storage as an Open State
        // SBP-M1 review: unnecessary clones, use & for keys
        ChallengeRequests::<T>::insert(
            // SBP-M1 review: unwrap may panic, should be handled gracefully
            pair.0.clone().unwrap(),
            pair.1.clone().unwrap(),
            Challenge {
                challenger: challenger.clone(),
                challenge_state: ChallengeState::Open,
            },
        );

        // SBP-M1 review: unnecessary clone and .to_vec()
        Self::deposit_event(Event::Challenge {
            challenger: challenger,
            // SBP-M1 review: unwrap may panic, should be handled gracefully
            challenged: pair.0.unwrap(),
            cid: pair.1.unwrap().to_vec(),
            state: ChallengeState::Open,
        });
        Ok(())
    }

    // SBP-M1 review: should probably be pub(crate) at most
    // SBP-M1 review: function too long, needs refactoring
    pub fn do_verify_challenge(
        challenged: &T::AccountId,
        cids: &Vec<CIDOf<T>>,
        pool_id: PoolIdOf<T>,
        class_id: ClassId,
        asset_id: AssetId,
    ) -> DispatchResult {
        // Validations made to verify some parameters given
        ensure!(
            // SBP-M1 review: refactor trait to borrow to determine membership
            T::Pool::is_member(challenged.clone(), pool_id),
            Error::<T>::AccountNotInPool
        );

        // SBP-M1 review: typo > 'auxiliary'?
        // Auxiliar structures
        let mut successful_cids = Vec::new();
        let mut failed_cids = Vec::new();

        // Validates if the challenge is successful given the cids given and the open challenge cid
        // SBP-M1 review: iteration should be bounded, accumulated challenge requests could exceed block limits
        // SBP-M1 review: use .iter_key_prefix(&challenged) to only iterate over challenges relating to challenged account
        // SBP-M1 review: destructure item into clear variable names
        for item in ChallengeRequests::<T>::iter() {
            // Checks if the user is the challenged account
            // SBP-M1 review: use let-else with `continue` to reduce nesting, or add .filter() to .iter()
            if item.0.clone() == challenged.clone() {
                let cid = &item.1;
                //Checks that the pool_id + account  + cid exists in the manifests storer data
                let mut data =
                    Self::manifests_storage_data((pool_id, &challenged, &item.1))
                        .ok_or(Error::<T>::ManifestStorerDataNotFound)?;
                let state;
                // Verifies if the cids provided contain the cid of the challenge to determine if it's a successful or failed challenge
                // SBP-M1 review: consider match statement
                if cids.contains(&item.1) {
                    state = ChallengeState::Successful;
                    successful_cids.push(item.1.to_vec());
                    // Mint the challenge tokens corresponding to the cid file size
                    // SBP-M1 review: initialise amount as result of match (e.g. `let amount = match .. {}`)
                    let mut amount = 0;
                    if let Some(file_check) = Manifests::<T>::get(pool_id, &cid) {
                        // SBP-M1 review: consider moving multiplication factor to constant or runtime config
                        // SBP-M1 review: use match statement
                        if let Some(file_size) = file_check.size {
                            // SBP-M1 review: use safe math
                            amount = file_size * 10;
                        } else {
                            amount = 10;
                        }
                    }
                    // Minted the challenge tokens
                    // SBP-M1 review: return value not checked for error
                    let _value = sugarfunge_asset::Pallet::<T>::do_mint(
                        challenged,
                        challenged,
                        class_id.into(),
                        asset_id.into(),
                        amount as u128,
                    );

                    // update claimed data
                    Self::update_claim_data(challenged, 0, 0, amount as u128)
                } else {
                    state = ChallengeState::Failed;
                    failed_cids.push(item.1.to_vec())
                }

                // Once the mint happens, the challenge is removed
                ChallengeRequests::<T>::remove(challenged, &cid);

                // The latest state of the cid is stored in the manifests storer data storage
                data.challenge_state = state;
                ManifestsStorerData::<T>::insert((pool_id, challenged, &cid), data);
            }
        }
        // SBP-M1 review: unnecessary clone
        Self::deposit_event(Event::VerifiedChallenges {
            challenged: challenged.clone(),
            successful: successful_cids,
            failed: failed_cids,
        });
        Ok(())
    }

    // SBP-M1 review: should probably be pub(crate) at most
    pub fn update_claim_data(
        account: &T::AccountId,
        minted_labor_tokens: MintBalance,
        expected_labor_tokens: MintBalance,
        challenge_tokens: MintBalance,
    ) {
        // SBP-M1 review: use contains_key() as cheaper, value not used
        // SBP-M1 review: use Claims::mutate(..) with inner match statement to upsert
        // SBP-M1 review: use match statement
        if Claims::<T>::try_get(&account).is_ok() {
            // SBP-M1 review: re-querying after just checked
            if let Some(mut value) = Self::claims(&account) {
                // SBP-M1 review: use safe math
                value.minted_labor_tokens += minted_labor_tokens;
                value.expected_labor_tokens += expected_labor_tokens;
                value.challenge_tokens += challenge_tokens;
                Claims::<T>::insert(&account, value);
            }
        } else {
            Claims::<T>::insert(
                &account,
                ClaimData {
                    minted_labor_tokens,
                    expected_labor_tokens,
                    challenge_tokens,
                },
            )
        }
    }

    // SBP-M1 review: should probably be pub(crate) at most
    pub fn get_network_size() -> f64 {
        // SBP-M1 review: loss of precision cast
        // Returns the value of the network size value
        NetworkSize::<T>::get() as f64
    }

    // SBP-M1 review: should probably be pub(crate) at most
    // SBP-M1 review: function too long, needs refactoring
    pub fn do_mint_labor_tokens(
        account: T::AccountId,
        class_id: ClassId,
        asset_id: AssetId,
        amount: MintBalance,
    ) -> DispatchResult {
        // SBP-M1 review: typo > 'auxiliary'?
        // Auxiliar structures
        // SBP-M1 review: floating point usage may not be deterministic, using sp_arithmetic::per_things is preferred
        let mut mining_rewards: f64 = 0.0;
        let mut storage_rewards: f64 = 0.0;

        // SBP-M1 review: bound iteration to prevent from exceeding block limits
        // Iterate over the manifest storer data to update the labor tokens to be minted
        // SBP-M1 review: consider changing order of storage map key, so you can use .iter_key_prefix() to only iterate for caller account, constraining data processed per call
        // SBP-M1 review: destructure manifest into clearer variables
        for manifest in ManifestsStorerData::<T>::iter() {
            // SBP-M1 review: floating point usage may not be deterministic, using sp_arithmetic::per_things is preferred
            let mut file_participation = 0.0;

            //Checks for the account
            // SBP-M1 review: unnecessary clones
            // SBP-M1 review: use let-else to reduce nesting or add .filter() to .iter()
            if account.clone() == manifest.0 .1.clone() {
                // Store the data to be updated later in the manifests storer data
                // SBP-M1 review: why re-query when you already have the manifest in memory from outer loop
                let mut updated_data = ManifestsStorerData::<T>::get((
                    manifest.0 .0,
                    &manifest.0 .1,
                    &manifest.0 .2,
                ))
                // SBP-M1 review: unwrap may panic, should be handled gracefully
                .unwrap();

                // Checks that the manifest has the file_size available to calculate the file participation in the network
                if let Some(file_check) = Manifests::<T>::get(manifest.0 .0, &manifest.0 .2)
                {
                    if let Some(file_size) = file_check.size {
                        // SBP-M1 review: use safe math
                        // SBP-M1 review: loss of precision in cast
                        file_participation = file_size as f64 / Self::get_network_size();
                    }
                }
                // If the state of the manifest is successful
                // SBP-M1 review: match statement might be cleaner
                if manifest.1.challenge_state == ChallengeState::Successful {
                    // When the active cycles reached {NUMBER_CYCLES_TO_ADVANCE} which is equal to 1 day, the manifest active days are increased and the rewards are calculated
                    // SBP-M1 review: use match statement
                    if manifest.1.active_cycles >= NUMBER_CYCLES_TO_ADVANCE {
                        // SBP-M1 review: use safe math
                        let active_days = manifest.1.active_days + 1;

                        // The calculation of the storage rewards
                        // SBP-M1 review: use 1.0 or 1_f64 instead > use cargo clippy
                        // SBP-M1 review: use safe math
                        // SBP-M1 review: floating point arithmetic
                        storage_rewards += (1 as f64
                            / (1 as f64 + exp(-0.1 * (active_days - 45) as f64)))
                            * DAILY_TOKENS_STORAGE
                            * file_participation;

                        // The calculation of the mining rewards
                        // SBP-M1 review: unnecessary cast > use cargo clippy
                        // SBP-M1 review: use safe math
                        // SBP-M1 review: floating point arithmetic
                        mining_rewards += DAILY_TOKENS_MINING as f64 * file_participation;

                        // SBP-M1 review: use safe math to avoid overflow panic
                        updated_data.active_days += 1;
                        updated_data.active_cycles = 0;
                    } else {
                        // SBP-M1 review: use safe math to avoid overflow panic
                        updated_data.active_cycles += 1;
                    }
                } else {
                    // SBP-M1 review: will this trigger if challenge_state == ChallengeState::Open?
                    // If the verification of the IPFS File failed {NUMBER_CYCLES_TO_RESET} times, the active_days are reset to 0
                    // SBP-M1 review: use match statement
                    if manifest.1.missed_cycles >= NUMBER_CYCLES_TO_RESET {
                        updated_data.missed_cycles = 0;
                        updated_data.active_days = 0;
                    } else {
                        // If the times failed are lower, the missed cycles are increased
                        // SBP-M1 review: use safe math
                        updated_data.missed_cycles += 1;
                    }
                }
                // Update the variables values for the next cycle
                ManifestsStorerData::<T>::mutate(
                    (manifest.0 .0, &account, manifest.0 .2),
                    |value| -> DispatchResult {
                        if let Some(manifest) = value {
                            manifest.active_cycles = updated_data.active_cycles;
                            manifest.missed_cycles = updated_data.missed_cycles;
                            manifest.active_days = updated_data.active_days;
                        }
                        Ok(())
                    },
                )?;
            }
        }

        // Calculate the total amount of rewards for the cycle
        // SBP-M1 review: floating point arithmetic
        let calculated_amount = mining_rewards + storage_rewards;

        // SBP-M1 review: return value not checked for error
        // SBP-M1 review: remove/action todo
        // TO DO: Here would be the call to mint the labor tokens
        let _value = sugarfunge_asset::Pallet::<T>::do_mint(
            &account,
            &account,
            class_id.into(),
            asset_id.into(),
            amount,
        );

        // SBP-M1 review: loss of sign, truncation by casting
        Self::update_claim_data(&account, amount, calculated_amount as u128, 0);

        Self::deposit_event(Event::MintedLaborTokens {
            account,
            class_id,
            asset_id,
            amount,
            // SBP-M1 review: loss of sign, truncation by casting
            calculated_amount: calculated_amount as u128,
        });
        Ok(())
    }

    // SBP-M1 review: should probably be pub(crate) at most
    pub fn do_update_size(
        storer: &T::AccountId,
        pool_id: PoolIdOf<T>,
        cid: &CIDOf<T>,
        size: FileSize,
    ) -> DispatchResult {
        // Validations made to verify some parameters given
        ensure!(
            // SBP-M1 review: borrow to avoid clone
            T::Pool::is_member(storer.clone(), pool_id),
            Error::<T>::AccountNotInPool
        );
        ensure!(
            Manifests::<T>::contains_key(pool_id, &cid),
            Error::<T>::ManifestNotFound
        );

        // Updated the file_size of a manifest
        Manifests::<T>::mutate(pool_id, &cid, |value| -> DispatchResult {
            // SBP-M1 review: use let-else to reduce nesting
            if let Some(manifest) = value {
                // Checks if there is a uploader that has the storer on chain
                if  Self::get_uploader_index_given_storer(&manifest.users_data, &storer).is_some() {
                    manifest.size = Some(size);
                    //Update the network size for the total amount of people that had that manifest before the file_size was given
                    NetworkSize::<T>::mutate(|total_value| {
                        *total_value += size * Self::get_total_storers(manifest.users_data.clone());
                    });
                }
            }
            Ok(())
        })?;

        // SBP-M1 review: wasteful clones and to_vec() > use cargo clippy
        Self::deposit_event(Event::UpdateFileSizeOutput {
            account: storer.clone(),
            pool_id: pool_id.clone(),
            cid: cid.to_vec(),
            size,
        });

        Ok(())
    }

    // SBP-M1 review: should probably be pub(crate) at most
    pub fn do_update_sizes(
        storer: T::AccountId,
        pool_id: PoolIdOf<T>,
        cids: Vec<CIDOf<T>>,
        sizes: Vec<FileSize>,
    ) -> DispatchResult {
        // The cycle to execute multiple times the update_size for the manifests file_size
        // SBP-M1 review: loop should be bounded to not exceed block limits (BoundedVec and benchmarking)
        // SBP-M1 review: use 'for (i, cid) in cids.into_iter().enumerate() {}' to simplify and avoid cids[i] and clone
        let n = cids.len();
        for i in 0..n {
            // SBP-M1 review: implicit clone
            let cid = cids[i].to_owned();
            // SBP-M1 review: indexing may panic
            let size = sizes[i].to_owned();
            Self::do_update_size(&storer, pool_id, &cid, size)?;
        }

        // SBP-M1 review: wasteful clones > use cargo clippy
        Self::deposit_event(Event::UpdateFileSizesOutput {
            account: storer.clone(),
            cids: Self::transform_cid_to_vec(cids),
            pool_id: pool_id.clone(),
            sizes: Self::transform_filesize_to_u64(sizes),
        });
        Ok(())
    }

    // function to get the next available uploader index for when a user is going to become a storer
    // SBP-M1 review: should probably be pub(crate) at most
    pub fn get_next_available_uploader_index(
        data: &Vec<UploaderData<T::AccountId>>,
    ) -> Option<usize> {
        return data
            .iter()
            .position(|x| x.replication_factor.saturating_sub(x.storers.len() as u16) > 0);
    }

    // Get the final replication factor if there is multiple uploaders in a manifest
    // SBP-M1 review: should probably be pub(crate) at most
    pub fn get_added_replication_factor(data: &Vec<UploaderData<T::AccountId>>) -> u16 {
        let mut result:u16 = 0;
        for user_data in data {          
            let diff = user_data.replication_factor as i32 - user_data.storers.len() as i32;
            result = result.saturating_add(diff as u16);
        }
        result
    }

    // Get the uploader index given the uploader account
    // SBP-M1 review: should probably be pub(crate) at most
    pub fn get_uploader_index(
        data: &Vec<UploaderData<T::AccountId>>,
        account: &T::AccountId,
    ) -> Option<usize> {
        return data.iter().position(|x| x.uploader == *account);
    }

    // Verifies if the account given is an uploader
    // SBP-M1 review: should probably be pub(crate) at most
    pub fn verify_account_is_uploader(
        data: &Vec<UploaderData<T::AccountId>>,
        account: &T::AccountId,
    ) -> bool {
        return data.iter().any(|x| x.uploader == *account);
    }

    // Get the uploader index given an storer account
    // SBP-M1 review: should probably be pub(crate) at most
    pub fn get_uploader_index_given_storer(
        data: &Vec<UploaderData<T::AccountId>>,
        account: &T::AccountId,
    ) -> Option<usize> {
        return data.iter().position(|x| x.storers.contains(account));
    }

    // Verify that the account is a storer
    // SBP-M1 review: should probably be pub(crate) at most
    pub fn verify_account_is_storer(
        data: &Vec<UploaderData<T::AccountId>>,
        account: &T::AccountId,
    ) -> bool {
        return data
            .iter()
            .any(|x| x.storers.contains(account));
    }

    // Get the storer index given the storer account and the uploader index
    // SBP-M1 review: should probably be pub(crate) at most
    pub fn get_storer_index(
        data: &Vec<UploaderData<T::AccountId>>,
        uploader_index: usize,
        account: &T::AccountId,
    ) -> Option<usize> {
        return data.get(uploader_index)
            .and_then(|uploader_data| uploader_data.storers.iter()
            .position(|x| *x == account.clone()));
    }

    // Get the total amount of storers given the uploaders vector
    pub(crate) fn get_total_storers(data: Vec<UploaderData<T::AccountId>>) -> u64 {
        let mut total_storers:u64 = 0;

        for uploader in data {
            total_storers = total_storers.saturating_add(uploader.storers.len() as u64);
        }

        total_storers
    }

    // Some functions to transform some values into another inside a Vec
    fn transform_manifest_to_vec(in_vec: Vec<ManifestMetadataOf<T>>) -> Vec<Vec<u8>> {
        in_vec
            .into_iter()
            .map(|manifest| manifest.into_inner())
            .collect()
    }

    fn transform_cid_to_vec(in_vec: Vec<CIDOf<T>>) -> Vec<Vec<u8>> {
        // SBP-M1 review: use .into_inner()
        in_vec.into_iter().map(|cid| cid.to_vec()).collect()
    }

    fn transform_filesize_to_u64(in_vec: Vec<FileSize>) -> Vec<u64> {
        in_vec.into_iter().collect()
    }
}
