#![feature(proc_macro_hygiene)]

#[macro_use] extern crate rocket;

#[cfg(test)] mod tests;


extern crate libra_wallet;
extern crate client;
//extern crate failure;

const CLIENT_WALLET_MNEMONIC_FILE:&str = "client.mnemonic";
use client::{client_front::ClientFront};
//pub use failure::Erorr;

use rocket::request::State;
use std::sync::Mutex;

//pub type Result<T> = std::result::Result<T, Error>;

pub struct  FrontController {
    client : Mutex<ClientFront>,
}
impl FrontController {
    pub fn new() -> Self{

        let host = "ac.testnet.libra.org";
        let port = "8000";
        let validator_set_file = "/home/dofork/Code/libra/rocket/examples/libra/trusted_peers.config.toml";

        let client = Mutex::new(
            ClientFront::new(
                &host,
                &port,
                &validator_set_file,
                &"",//&faucet_account_file,
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

    pub fn mint(&self,receiver_address_decoded:String,num_coins:String)
    {
        let mut client = self.client.lock().unwrap();
        client.mint_coins_v2(receiver_address_decoded,num_coins,true);
    }

    pub fn get_balance(&self,account_address_decoded:String) -> (u64)
    {
        let mut client = self.client.lock().unwrap();
        client.get_balance_v2(account_address_decoded).unwrap()
    }

    pub fn transfer_coins(
        &self,sender_address : String,
        receiver_address : String,
        coins : String,
        _gas_unit_price : u64,
        _max_gas : u64
    ) {

        let mut client = self.client.lock().unwrap();
        client.transfer_coins_v2(&sender_address, &receiver_address, &coins,None, None, true);
    }

    pub fn recovery_wallet(
        &self,
        mnemonic : String
    ) -> (String) {
        let mut client = self.client.lock().unwrap();
        match client.recovery_wallet_v2(&mnemonic){
            Ok(address) => { address }
            Err(_error) => { "".to_string() }
        }
    }

}

#[get("/create_account")]
fn create_account(controller : State<FrontController>) -> String
{
    let (mnemonic,pubkey) = controller.create_account();
    format!("{{\"mnemonic\":\"{}\",\"pubkey\":\"{}\" }}", mnemonic, pubkey)

}

#[get("/mint/<receiver_address>/<num_coins>")]
fn mint(controller : State<FrontController> ,receiver_address:String,num_coins:String) -> String
{
    controller.mint(receiver_address,num_coins);
    "mint finished".to_string()
}

#[get("/get_balance/<address>")]
fn get_balance(controller:State<FrontController>,address:String) -> String
{
    controller.get_balance(address).to_string()
}

#[get("/transfer_coins/<sender_address>/<receiver_address>/<coins>/<gas_unit_price>/<max_gas>")]
fn transfer_coins(
    controller : State<FrontController>,
    sender_address : String,
    receiver_address : String,
    coins : String,
    gas_unit_price : u64,
    max_gas : u64
) {
    controller.transfer_coins(sender_address,receiver_address,coins,gas_unit_price,max_gas);
}

#[get("/recovery_wallet/<mnemonic>")]
fn recovery_wallet(
    controller : State<FrontController>,
    mnemonic : String
) -> String {
    controller.recovery_wallet(mnemonic)
}

fn main() {
    let controller = FrontController::new();
    rocket::ignite().manage(controller).mount("/", routes![create_account,get_balance,mint,transfer_coins,recovery_wallet]).launch();
}
