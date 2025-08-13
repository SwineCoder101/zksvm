use {
    solana_client::rpc_client::RpcClient,
    solana_sdk::{
        account::{Account, AccountSharedData, ReadableAccount},
        pubkey::Pubkey,
    },
    solana_svm::transaction_processing_callback::TransactionProcessingCallback,
    std::{collections::HashMap, sync::RwLock},
    solana_svm_callback::InvokeContextCallback,
    solana_sdk::precompiles::PrecompileError
};

impl InvokeContextCallback for RollupAccountLoader<'_> {
    fn get_epoch_stake(&self) -> u64 {
        0 // Stub implementation
    }

    fn get_epoch_stake_for_vote_account(&self, _vote_address: &Pubkey) -> u64 {
        0 // Stub implementation
    }

    fn is_precompile(&self, _program_id: &Pubkey) -> bool {
        false // Stub implementation
    }

    fn process_precompile(
        &self,
        _program_id: &Pubkey,
        _data: &[u8],
        _instruction_datas: Vec<&[u8]>,
    ) -> Result<(), PrecompileError> {
        Err(PrecompileError::InvalidPublicKey) // Stub implementation
    }
}

pub struct RollupAccountLoader<'a>{
    pub cache: RwLock<HashMap<Pubkey,AccountSharedData>>,
    pub rpc_client: &'a RpcClient,
}

impl<'a> RollupAccountLoader<'a>  {
    pub fn new(rpc_client: &'a RpcClient)->Self{
        Self { cache: RwLock::new(HashMap::new()), rpc_client }
    }

    pub fn add_account(&mut self,pubkey:Pubkey,modified_new_accounts:AccountSharedData){
        let mut map = self.cache.write().unwrap();
        map.insert(pubkey, modified_new_accounts);
        log::info!("updated account in cache: {:?}", map);
    }
}

/// Implementation of the SVM API's `TransactionProcessingCallback` interface.
impl TransactionProcessingCallback for RollupAccountLoader<'_>{
    fn get_account_shared_data(&self, pubkey: &Pubkey) -> Option<AccountSharedData> {
        //check the local cache first
        if let Some(account) = self.cache.read().unwrap().get(pubkey){
            log::info!("Account {} loaded from cache", pubkey);
            return Some(account.clone());
        }

        //not in cache, fetch from the base chain (solana)
        match self.rpc_client.get_account(pubkey){
            Ok(account)=>{
                let account_data: AccountSharedData = account.into();

                //storing the fetched account in the cache for next time.
                self.cache.write().unwrap().insert(*pubkey, account_data.clone());
                Some(account_data)
            }
            Err(_) => None,
        }
    }
    fn account_matches_owners(&self, account: &Pubkey, owners: &[Pubkey]) -> Option<usize> {
        self.get_account_shared_data(account).and_then(|account| owners.iter().position(|key| account.owner().eq(key)))
    }

    fn add_builtin_account(&self, _name: &str, _program_id: &Pubkey) {
        // Not needed for your rollup, can be empty
    }

    fn get_current_epoch_vote_account_stake(&self, _vote_address: &Pubkey) -> u64 {
        0 // Stub implementation
    }
}