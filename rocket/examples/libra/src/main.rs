#![feature(proc_macro_hygiene)]

#[macro_use] extern crate rocket;

#[cfg(test)] mod tests;


extern crate libra_wallet;
extern crate client;

const CLIENT_WALLET_MNEMONIC_FILE:&str = "client.mnemonic";
use libra_wallet::{io_utils, wallet_library::WalletLibrary};
use client::{client_front::ClientFront};

use rocket::request::State;
use rocket::{Request, Data, Response};
use rocket::http::{Method, ContentType, Status};
use std::sync::Mutex;



pub struct User {
    /// Wallet libray managing user account
    wallet: Mutex<WalletLibrary>,
}

impl  User {

    pub fn new() -> Self {
        let wallet = Mutex::new(WalletLibrary::new()); //请看get_libra_wallet后半部分
        User{wallet,}
    }

    pub fn get_mnemonic(& self) -> String {
        let mut wlt =self.wallet.lock().unwrap();
        let mnemonic = wlt.mnemonic();
        wlt.new_address();
        mnemonic
    }

    pub fn get_publick_key(& self) -> String {
        let wlt =self.wallet.lock().unwrap();
        let addresses = wlt.get_addresses();
        match addresses {
            Ok(addresses) => {
                let address = addresses[0];

                address.short_str()
            }
            Err(_error) => {
                "get_publick_key error".to_string()
            }
        }
    }

}

pub struct  FrontController {
    client : Mutex<ClientFront>,
}
impl FrontController {
    pub fn new() -> Self{

        let host = "ac.testnet.libra.org";
        let port = "8000";
        let validator_set_file = "/home/dofork/CLionProjects/rocket/examples/libra/trusted_peers.config.toml";

        let client = Mutex::new(
            ClientFront::new(
                &host,
                &port,
                &validator_set_file,
                &"",//&faucet_account_file,
                false,//args.sync,
                None,//args.faucet_server,
                None,//args.mnemonic_file,
            ).unwrap()
        );
        FrontController{client,}
    }

    pub fn create_account(&self) -> (String,String)
    {
        let mut client = self.client.lock().unwrap();
        match client.create_account() {
            Ok((mnemonic,pubkey)) => {
                (mnemonic,pubkey)
            }
            Err(_error) => {
                ("".to_string(),"".to_string())
            }
        }
    }

    pub fn mint(&self,receiver_address_decoded:String,num_coins:u64)
    {
        let mut client = self.client.lock().unwrap();
        client.mint_coins_v2(receiver_address_decoded,num_coins,true);
    }

    pub fn transfer_coins(
        &self,sender_address : String,
        receiver_address : String,
        coins : u64,
        gas_unit_price : u64,
        max_gas : u64
    ) {

        let mut client = self.client.lock().unwrap();
        client.transfer_coins_v2(&sender_address, &receiver_address, coins, Option::from(gas_unit_price), Option::from(max_gas), true);
    }

}

#[get("/create_account")]
fn create_account(controller : State<FrontController>) -> String
{
    let (mnemonic,pubkey) = controller.create_account();
    format!("{{\"mnemonic\":\"{}\",\"pubkey\":\"{}\" }}", mnemonic, pubkey)
}

#[get("/mine/<receiver_address>/num_coins")]
fn mint(controller : State<FrontController> ) -> String
{
    controller.mint(receiver_address,num_coins);
    "mint finished!"
}


#[get("/transfer_coins/<sender_address>/<receiver_address>/<coins>/<gas_unit_price>/<max_gas>")]
fn transfer_coins(
    controller : State<FrontController>,
    sender_address : String,
    receiver_address : String,
    coins : u64,
    gas_unit_price : u64,
    max_gas : u64
) {
    controller.transfer_coins(sender_address,receiver_address,coins,gas_unit_price,max_gas);
}



fn main() {

    let controller = FrontController::new();
    rocket::ignite().manage(controller).mount("/", routes![create_account]).launch();
}
