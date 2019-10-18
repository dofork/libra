// Copyright (c) The Libra Core Contributors
// SPDX-License-Identifier: Apache-2.0


use crate::{commands::*, grpc_client::GRPCClient, AccountData, AccountStatus};
use admission_control_proto::proto::admission_control::SubmitTransactionRequest;
use config::{config::PersistableConfig, trusted_peers::ConsensusPeersConfig};
use crypto::{ed25519::*, test_utils::KeyPair};
use failure::prelude::*;
use futures::{future::Future, stream::Stream};
use libra_wallet::{io_utils, wallet_library::WalletLibrary};
use logger::prelude::*;
use num_traits::{
    cast::{FromPrimitive, ToPrimitive},
    identities::Zero,
};
use libra_wallet::{mnemonic::Mnemonic};
use proto_conv::IntoProto;
use rust_decimal::Decimal;
use std::{
    collections::{ HashMap},
    convert::TryFrom,
     fs,
    io::{stdout,   Write},
    path::{ Path,PathBuf},
    str::{self, FromStr},
    sync::Arc,
    thread, time,
};
use tools::tempdir::TempPath;
use types::{
    account_address::{AccountAddress, ADDRESS_LENGTH},
    account_config::{
          association_address,
         get_account_resource_or_default, AccountResource,
    },
    account_state_blob::{AccountStateBlob},

    transaction::{
         Program, Version,Script,TransactionPayload
    },
    transaction_helpers::{create_signed_txn, TransactionSigner},
    validator_verifier::ValidatorVerifier,
};

const CLIENT_WALLET_MNEMONIC_FILE: &str = "client.mnemonic";
const GAS_UNIT_PRICE: u64 = 0;
const MAX_GAS_AMOUNT: u64 = 100_000;
const TX_EXPIRATION: i64 = 100;


/// each User represent a user
pub struct User {
    wallet : WalletLibrary,
    accounts: Vec<AccountData>,
}



/// Proxy handling CLI commands/inputs.
pub struct ClientFront {
    /// client for admission control interface.
    pub client: GRPCClient,
    /// Created accounts.
    pub accounts: Vec<AccountData>,
    /// Address to account_ref_id map.
    address_to_ref_id: HashMap<AccountAddress, usize>,
    /// Host that operates a faucet service
    faucet_server: String,
    /// Account used for mint operations.
    pub faucet_account: Option<AccountData>,
    /// Wallet library managing user accounts.
    wallet: WalletLibrary,
    /// temp files (alive for duration of program)
    temp_files: Vec<PathBuf>,
    /// host users
    users:Vec<User>,
}

impl ClientFront {
    /// Construct a new TestClient.
    pub fn new(
        host: &str,
        ac_port: u16,
        validator_set_file: &str,
        faucet_account_file: &str,
        faucet_server: Option<String>,
        mnemonic_file: Option<String>,
    ) -> Result<Self> {
        let validators = ConsensusPeersConfig::load_config(Path::new(validator_set_file)).peers;
        ensure!(
            !validators.is_empty(),
            "Not able to load validators from trusted peers config!"
        );
        // Total 3f + 1 validators, 2f + 1 correct signatures are required.
        // If < 4 validators, all validators have to agree.
        let validator_pubkeys: HashMap<AccountAddress, Ed25519PublicKey> = validators
            .into_iter()
            .map(|(peer_id_str, peer_info)| {
                (
                    AccountAddress::from_str(&peer_id_str).unwrap(),
                    peer_info.consensus_pubkey,
                )
            })
            .collect();
        let validator_verifier = Arc::new(ValidatorVerifier::new(validator_pubkeys));
        let client = GRPCClient::new(host, ac_port, validator_verifier)?;

        let accounts = vec![];

        // If we have a faucet account file, then load it to get the keypair
        let faucet_account = if faucet_account_file.is_empty() {
            None
        } else {
            let faucet_account_keypair: KeyPair<Ed25519PrivateKey, Ed25519PublicKey> =
                ClientFront::load_faucet_account_file(faucet_account_file);
            let faucet_account_data = Self::get_account_data_from_address(
                &client,
                association_address(),
                true,
                Some(KeyPair::<Ed25519PrivateKey, _>::from(
                    faucet_account_keypair.private_key,
                )),
            )?;
            // Load the keypair from file
            Some(faucet_account_data)
        };

        let faucet_server = match faucet_server {
            Some(server) => server.to_string(),
            None => host.replace("ac", "faucet"),
        };

        let address_to_ref_id = accounts
            .iter()
            .enumerate()
            .map(|(ref_id, acc_data): (usize, &AccountData)| (acc_data.address, ref_id))
            .collect::<HashMap<AccountAddress, usize>>();

        Ok(ClientFront {
            client,
            accounts,
            address_to_ref_id,
            faucet_server,
            faucet_account,
            wallet: Self::get_libra_wallet(mnemonic_file)?,
            temp_files: vec![],
            users:vec![],
        })
    }


    fn get_account_ref_id(&self, sender_account_address: &AccountAddress) -> Result<usize> {
        Ok(*self
            .address_to_ref_id
            .get(&sender_account_address)
            .ok_or_else(|| {
                format_err!(
                    "Unable to find existing managing account by address: {}, to see all existing \
                     accounts, run: 'account list'",
                    sender_account_address
                )
            })?)
    }


    /// Get balance from validator for the account specified.
    pub fn get_balance_v2(&mut self,account_address_decoded:String) -> Result<u64> {
        let address = ClientFront::address_from_strings(&account_address_decoded)?;
        self.get_account_resource_and_update(address).map(|res| {
            let whole_num = res.balance() / 1_000_000;
            //let remainder = res.balance() % 1_000_000;
            //format!("{}.{:0>6}", whole_num.to_string(), remainder.to_string())
            whole_num
        })
    }



    ///Mints coins for the receiver specified version 2
    pub fn mint_coins_v2(&mut self,receiver_address_decoded:String,num_coins: String, is_blocking:bool) -> Result<()>{
        let receiver = ClientFront::address_from_strings(&receiver_address_decoded)?;
        let micro_num_coins = Self::convert_to_micro_libras(&num_coins)?;
        match self.faucet_account {
            Some(_) => self.mint_coins_with_local_faucet_account(&receiver, micro_num_coins, is_blocking),
            None => self.mint_coins_with_faucet_service(&receiver, micro_num_coins, is_blocking),
        }
    }

    /// convert number of Libras (main unit) given as string to number of micro Libras
    pub fn convert_to_micro_libras(input: &str) -> Result<u64> {
        ensure!(!input.is_empty(), "Empty input not allowed for libra unit");
        // This is not supposed to panic as it is used as constant here.
        let max_value = Decimal::from_u64(std::u64::MAX).unwrap() / Decimal::new(1_000_000, 0);
        let scale = input.find('.').unwrap_or(input.len() - 1);
        ensure!(
            scale <= 14,
            "Input value is too big: {:?}, max: {:?}",
            input,
            max_value
        );
        let original = Decimal::from_str(input)?;
        ensure!(
            original <= max_value,
            "Input value is too big: {:?}, max: {:?}",
            input,
            max_value
        );
        let value = original * Decimal::new(1_000_000, 0);
        ensure!(value.fract().is_zero(), "invalid value");
        value.to_u64().ok_or_else(|| format_err!("invalid value"))
    }

    /// Waits for the next transaction for a specific address and prints it
    pub fn wait_for_transaction(&mut self, account: AccountAddress, sequence_number: u64) {
        let mut max_iterations = 5000;
        print!("waiting ");
        loop {
            stdout().flush().unwrap();
            max_iterations -= 1;

            if let Ok(Some((_, Some(events)))) =
            self.client
                .get_txn_by_acc_seq(account, sequence_number - 1, true)
            {
                println!("transaction is stored!");
                if events.is_empty() {
                    println!("no events emitted");
                }
                break;
            } else if max_iterations == 0 {
                panic!("wait_for_transaction timeout");
            } else {
                print!(".");
            }
            thread::sleep(time::Duration::from_millis(10));
        }
    }
    /// create mnemonic for user
    pub fn generate_mnemonic (&mut self) -> Result<String>
    {
        let (mnemonic,address_human) = self.create_account().unwrap();
        Ok(mnemonic)
    }


    /// fetch a pub address of wallet of mnemonic
    pub fn fetch_address(&mut self,mnemonic_string : &String) -> Result<String>
    {
        self.recovery_wallet_v2(mnemonic_string)
    }

    /// create account for user
    pub fn create_account (&mut self) -> Result<(String,String)>
    {
        let mut wallet = WalletLibrary::new();
        let mnemonic = wallet.mnemonic();
        let (address,_child_number) = wallet.new_address()?;
        let address_human = hex::encode(address);
        let account_data = Self::get_account_data_from_address(&self.client,address,true,None)?;
        let a_user = User { wallet:wallet, accounts:vec![account_data],};
        self.users.push(a_user);
        Ok((mnemonic,address_human))
    }


    /*
     let child_private_1 = key_factory.private_child(ChildNumber(1)).unwrap();
    assert_eq!(
        "a325fe7d27b1b49f191cc03525951fec41b6ffa2d4b3007bb1d9dd353b7e56a6",
        hex::encode(&child_private_1.private_key.to_bytes()[..])
    );
    let child_key = self.key_factory.private_child(child.clone())?;
    let signature = child_key.sign(txn_hashvalue);
    let public_key = child_key.get_public();
    */

    /// Transfers coins from sender to receiver version2
	/// create by hebo
    pub fn transfer_coins_v2(
        &mut self,
        sender_address_strings: &String,
        receiver_address_strings: &String,
        num_coins: &String,
        gas_unit_price: Option<u64>,
        max_gas_amount: Option<u64>,
        _is_blocking:bool,
    ) -> Result<()>
    {

        let sender_address = ClientFront::address_from_strings(sender_address_strings)?;
        let receiver_address = ClientFront::address_from_strings(receiver_address_strings)?;

        let sender_account =
            Self::get_account_data_from_address(
                &self.client, sender_address,
                true,
                None /* key_pair */
            );
        let (wallet,sender_account) = self.wallet_of_account_data(sender_account.unwrap()).unwrap();
        let micro_num_coins = Self::convert_to_micro_libras(num_coins)?;
        match sender_account {
            Ok(sender_account) => {
                let program = transaction_builder::encode_transfer_script(&receiver_address, micro_num_coins);
                let req = self.create_submit_transaction_req(
                    wallet,
                    TransactionPayload::Script(program),
                    &sender_account, /* AccountData */
                    max_gas_amount, /* max_gas_amount */
                    gas_unit_price, /* gas_unit_price */
                )?;

                let mut sender_mut = sender_account;
                self.client.submit_transaction(Some(&mut sender_mut), &req)?;
                Ok(())
            }
            Err(_error) => {
                "can't get accountdata from account address".to_string();
                Err(_error)
            }
        }
    }
    /// recover_wallet_v2
        /// create by hebo
    pub fn recovery_wallet_v2(&mut self, mnemonic_string : &String) -> Result<(String)>
    {
        let mnemonic = Mnemonic::from(&mnemonic_string[..])?;
        let mut wallet = WalletLibrary::new_from_mnemonic(mnemonic);
        let (address,_child_number) = wallet.new_address()?;

        let account =
            Self::get_account_data_from_address(
                &self.client, address,
                true,
                None /* key_pair */
            );
        if let Err(_) = self.wallet_of_account_data(account.unwrap()) {
            let account_data = Self::get_account_data_from_address(&self.client,address,true,None)?;
            let a_user = User { wallet:wallet, accounts:vec![account_data],};
            self.users.push(a_user);
        };

        let address_human = hex::encode(address);
        Ok(address_human)
    }



    /// wallet of account
    fn wallet_of_account_data(&self,account_target:AccountData) -> Result<(&WalletLibrary,Result<AccountData>)>
    {
        for (_i,user) in self.users.iter().enumerate() {
            for (_i, account) in user.accounts.iter().enumerate() {
                if hex::encode(account.address) == hex::encode(account_target.address) {
                    return Ok((&user.wallet,Ok(account_target)))
                }
            }
        }
        Err(format_err!("No wallet maintained for the target accountdata"))
    }

    /// wallet for mnemonic
    fn wallet_of_mnemonic(&self,mnemonic:String) -> Result<&WalletLibrary>
    {
        for (_i,user) in self.users.iter().enumerate() {
            let wallet = &user.wallet;
            if wallet.mnemonic() == mnemonic {
                return Ok(&wallet)
            }
        }
        Err(format_err!("No wallet maintained for the mnemonic"))
    }

    /// Get account state from validator and update status of account if it is cached locally.
    fn get_account_state_and_update(
        &mut self,
        address: AccountAddress,
    ) -> Result<(Option<AccountStateBlob>, Version)> {
        let account_state = self.client.get_account_blob(address)?;
        if self.address_to_ref_id.contains_key(&address) {
            let account_ref_id = self
                .address_to_ref_id
                .get(&address)
                .expect("Should have the key");
            let mut account_data: &mut AccountData =
                self.accounts.get_mut(*account_ref_id).unwrap_or_else(|| panic!("Local cache not consistent, reference id {} not available in local accounts", account_ref_id));
            if account_state.0.is_some() {
                account_data.status = AccountStatus::Persisted;
            }
        };
        Ok(account_state)
    }

    /// Get account resource from validator and update status of account if it is cached locally.
    fn get_account_resource_and_update(
        &mut self,
        address: AccountAddress,
    ) -> Result<AccountResource> {
        let account_state = self.get_account_state_and_update(address)?;
        get_account_resource_or_default(&account_state.0)
    }

    /// Get account using specific address.
    /// Sync with validator for account sequence number in case it is already created on chain.
    /// This assumes we have a very low probability of mnemonic word conflict.
    fn get_account_data_from_address(
        client: &GRPCClient,
        address: AccountAddress,
        sync_with_validator: bool,
        key_pair: Option<KeyPair<Ed25519PrivateKey, Ed25519PublicKey>>,
    ) -> Result<AccountData> {
        let (sequence_number, status) = match sync_with_validator {
            true => match client.get_account_blob(address) {
                Ok(resp) => match resp.0 {
                    Some(account_state_blob) => (
                        get_account_resource_or_default(&Some(account_state_blob))?
                            .sequence_number(),
                        AccountStatus::Persisted,
                    ),
                    None => (0, AccountStatus::Local),
                },
                Err(e) => {
                    error!("Failed to get account state from validator, error: {:?}", e);
                    (0, AccountStatus::Unknown)
                }
            },
            false => (0, AccountStatus::Local),
        };
        Ok(AccountData {
            address,
            key_pair,
            sequence_number,
            status,
        })
    }


    fn load_faucet_account_file(
        faucet_account_file: &str,
    ) -> KeyPair<Ed25519PrivateKey, Ed25519PublicKey> {
        match fs::read(faucet_account_file) {
            Ok(data) => {
                bincode::deserialize(&data[..]).expect("Unable to deserialize faucet account file")
            }
            Err(e) => {
                panic!(
                    "Unable to read faucet account file: {}, {}",
                    faucet_account_file, e
                );
            }
        }
    }

    fn address_from_strings(data: &str) -> Result<AccountAddress> {
        let account_vec: Vec<u8> = hex::decode(data.parse::<String>()?)?;
        ensure!(
            account_vec.len() == ADDRESS_LENGTH,
            "The address {:?} is of invalid length. Addresses must be 32-bytes long"
        );
        let account = match AccountAddress::try_from(&account_vec[..]) {
            Ok(address) => address,
            Err(error) => bail!(
                "The address {:?} is invalid, error: {:?}",
                &account_vec,
                error,
            ),
        };
        Ok(account)
    }
    fn mint_coins_with_local_faucet_account(
        &mut self,
        receiver: &AccountAddress,
        num_coins: u64,
        is_blocking: bool,
    ) -> Result<()> {
        ensure!(self.faucet_account.is_some(), "No faucet account loaded");
        let sender = self.faucet_account.as_ref().unwrap();
        let sender_address = sender.address;
        let program = transaction_builder::encode_mint_script(&receiver, num_coins);
        let req = self.create_submit_transaction_req(
            &self.wallet,
            TransactionPayload::Script(program),
            sender,
            None, /* max_gas_amount */
            None, /* gas_unit_price */
        )?;
        let mut sender_mut = self.faucet_account.as_mut().unwrap();
        let resp = self.client.submit_transaction(Some(&mut sender_mut), &req);
        if is_blocking {
            self.wait_for_transaction(
                sender_address,
                self.faucet_account.as_ref().unwrap().sequence_number,
            );
        }
        resp
    }


    fn mint_coins_with_faucet_service(
        &mut self,
        receiver: &AccountAddress,
        num_coins: u64,
        is_blocking: bool,
    ) -> Result<()> {

        let client = reqwest::ClientBuilder::new().use_sys_proxy().build()?;

        let url = reqwest::Url::parse_with_params(
            format!("http://{}", self.faucet_server).as_str(),
            &[
                ("amount", num_coins.to_string().as_str()),
                ("address", format!("{:?}", receiver).as_str()),
            ],
        )?;

        let mut response = client.post(url).send()?;
        let status_code = response.status();
        let body = response.text()?;
        if !status_code.is_success() {
            return Err(format_err!(
                "Failed to query remote faucet server[status={}]: {:?}",
                status_code.as_str(),
                body,
            ));
        }
        let sequence_number = body.parse::<u64>()?;
        if is_blocking {
            self.wait_for_transaction(association_address(), sequence_number);
        }

        Ok(())
    }

    /// Craft a transaction request.
    fn create_submit_transaction_req(
        &self,
        wallet: &WalletLibrary,
        program: TransactionPayload,
        sender_account: &AccountData,
        max_gas_amount: Option<u64>,
        gas_unit_price: Option<u64>,
    ) -> Result<SubmitTransactionRequest> {
        let signer: Box<&dyn TransactionSigner> = match &sender_account.key_pair {
            Some(key_pair) => Box::new(key_pair),
            None => Box::new(wallet),
        };
        let signed_txn = create_signed_txn(
            *signer,
            program,
            sender_account.address,
            sender_account.sequence_number,
            max_gas_amount.unwrap_or(MAX_GAS_AMOUNT),
            gas_unit_price.unwrap_or(GAS_UNIT_PRICE),
            TX_EXPIRATION,
        )
            .unwrap();
        let mut req = SubmitTransactionRequest::new();
        req.set_signed_txn(signed_txn.into_proto());
        Ok(req)
    }

    fn mut_account_from_parameter(&mut self, para: &str) -> Result<&mut AccountData> {
        let account_ref_id = match is_address(para) {
            true => {
                let account_address = ClientFront::address_from_strings(para)?;
                *self
                    .address_to_ref_id
                    .get(&account_address)
                    .ok_or_else(|| {
                        format_err!(
                            "Unable to find local account by address: {:?}",
                            account_address
                        )
                    })?
            }
            false => para.parse::<usize>()?,
        };
        let account_data = self
            .accounts
            .get_mut(account_ref_id)
            .ok_or_else(|| format_err!("Unable to find account by ref id: {}", account_ref_id))?;
        Ok(account_data)
    }

    fn get_libra_wallet(mnemonic_file: Option<String>) -> Result<WalletLibrary> {
        let wallet_recovery_file_path = if let Some(input_mnemonic_word) = mnemonic_file {
            Path::new(&input_mnemonic_word).to_path_buf()
        } else {
            let mut file_path = std::env::current_dir()?;
            file_path.push(CLIENT_WALLET_MNEMONIC_FILE);
            file_path
        };

        let wallet = if let Ok(recovered_wallet) = io_utils::recover(&wallet_recovery_file_path) {
            recovered_wallet
        } else {
            let new_wallet = WalletLibrary::new();
            new_wallet.write_recovery(&wallet_recovery_file_path)?;
            new_wallet
        };
        Ok(wallet)
    }

}

