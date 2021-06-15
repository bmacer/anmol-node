use crate::{
    PendingNftOf,
    PendingNftQueueOf,
    Config,
    Error,
    pallet::{Call},
    local_storage::{LocalStorageValue, VecKey, get_nft_key_hash},
    ByteVector,
    utils::{remove_vector_item},
};
use codec::{Encode, Decode};
use sp_core::{
	crypto::KeyTypeId,
};
use frame_support::{
    pallet_prelude::{debug},
    RuntimeDebug,
};
use frame_system::{
    offchain::{Signer, SendSignedTransaction},
};

pub const KEY_TYPE: KeyTypeId = KeyTypeId(*b"_nft");

pub mod crypto {
	use super::KEY_TYPE;
	use sp_runtime::{
		app_crypto::{app_crypto, sr25519},
		MultiSignature, MultiSigner,
	};
	use frame_system::{
		offchain::{AppCrypto},
	};

	app_crypto!(sr25519, KEY_TYPE);

	pub struct TestAuthId;

	impl AppCrypto<MultiSigner, MultiSignature> for TestAuthId {
		type RuntimeAppPublic = Public;
		type GenericSignature = sp_core::sr25519::Signature;
		type GenericPublic = sp_core::sr25519::Public;
	}
}

#[derive(Encode, Decode, Clone, RuntimeDebug, Default)]
struct LocalNftMetadata<ClassId: Default> {
    mould_id: ClassId,
    dna: ByteVector,
}

type LocalNftMetadataOf<T> = LocalNftMetadata<<T as orml_nft::Config>::ClassId>;

const NEW_NFT_REQUESTS_KEY: &[u8] = b"new_nft_requests";
const NFT_PENDING_QUEUE: &[u8] = b"nft_pending_queue";

pub fn hook_init<T: Config>(block_number: T::BlockNumber) {
    debug::info!("--- offchain_worker block_number: {:?}", block_number);

    let new_nft_requests = LocalStorageValue::<PendingNftQueueOf<T>>::new(NEW_NFT_REQUESTS_KEY);
    let pending_nft_queue = LocalStorageValue::<PendingNftQueueOf<T>>::new(NFT_PENDING_QUEUE);

    let result = offchain_update_pending_nft_queue::<T>(pending_nft_queue.clone(), new_nft_requests);

    match result {
        Ok(_) => {
            if let Ok(queue) = pending_nft_queue.get::<T>() {
                if queue.len() > 0 {
                    debug::info!("--- Pending nft queue key: {:x}, value: {:?}", VecKey(NFT_PENDING_QUEUE.to_vec()), queue);

                    let pending_nft = &queue[0];
                    execute_nft_from_pending_queue::<T>(pending_nft.clone());

                    let _: Result<_, Error<T>> = pending_nft_queue.mutate(|x| remove_vector_item(x, pending_nft));
                }
            }
        },
        Err(x) => {
            debug::error!("--- result error: {:?}", x);
        }
    }
}

pub fn offchain_new_nft_requests_key() -> VecKey {
    let key = NEW_NFT_REQUESTS_KEY.to_vec();
    VecKey(key)
}

fn offchain_update_pending_nft_queue<T: Config>
    (
        pending_nft_queue: LocalStorageValue::<PendingNftQueueOf<T>>,
        new_nft_requests: LocalStorageValue::<PendingNftQueueOf<T>>
    ) -> Result<PendingNftQueueOf<T>, Error<T>> {
    pending_nft_queue.mutate(|mut current_pending_nft_queue| {
        let new_nft_requests = new_nft_requests.get()?;

        for v in new_nft_requests {
            current_pending_nft_queue.push(v);
        }
        Ok(current_pending_nft_queue)
    })
}

fn execute_nft_from_pending_queue<T: Config>(pending_nft: PendingNftOf<T>) {
    debug::RuntimeLogger::init();
    debug::info!("--- Execute nft from pending queue: {:?}", pending_nft);

    let key_hash = get_nft_key_hash::<T>(pending_nft.class_id, pending_nft.token_data.clone());
    let local_nft_metadata = LocalStorageValue::<LocalNftMetadataOf<T>>::new(&key_hash);

    if let Ok(value) = local_nft_metadata.get::<T>() {
        debug::error!("--- Error: local_nft_metadata already exist: {:?}", value);
        return
    }

    let mint_nft_closure = |_: &frame_system::offchain::Account<T>| return Call::mint_nft(key_hash, pending_nft.clone());
    if let Ok(()) = send_signed(mint_nft_closure) {
        let metadata = LocalNftMetadata {
            mould_id: pending_nft.class_id,
            dna: pending_nft.token_data.dna,
        };

        local_nft_metadata.set(&metadata);
    }
}

fn send_signed<T: Config>(call_closure: impl Fn(&frame_system::offchain::Account<T>) -> Call<T>) -> Result<(), Error<T>> {
    let signer = Signer::<T, T::AuthorityId>::any_account();
    let result = signer.send_signed_transaction(call_closure);

    if let Some((acc, res)) = result {
        if res.is_err() {
            debug::error!("--- Send signed - Error: {:?}, account id: {:?}", res, acc.id);
            return Err(Error::<T>::OffchainSignedTxError)
        }

        debug::info!("--- Send signed - Ok");
        return Ok(());
    } 

    debug::error!("--- Send signed - No local account available");
    return Err(Error::<T>::NoLocalAccountForSigning);
}