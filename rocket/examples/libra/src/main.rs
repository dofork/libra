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
use std::fmt::Error;

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

    pub fn create_account(&self) -> Result<(String,String)failure::error::Error>
    {
        let mut client = self.client.lock().unwrap();
        let (mnemonic,pubkey) = client.create_account()?;
        Ok((mnemonic,pubkey))
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

#[get("/generate_mnemonic")]
fn generate_mnemonic( user: State< User>) -> String {
    user.get_mnemonic()
}

#[get("/get_private_key")]
fn get_private_key(user: State<User>) -> String {
    user.get_publick_key()
}


#[get("/get_public_key")]
fn get_public_key() -> &'static str {
    "Hello, world!"
}

#[get("/create_account")]
fn create_account(controller : State<FrontController>) -> String
{
    let (mnemonic,pubkey) = controller.create_account().unwrap();
    "{\"mnemonic\":\""   +  mnemonic  +  "\"，\"pubkey\":\"   +pubkey+   \"}"
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
    //let user = User::new();
    //rocket::ignite().manage(user).manage(client).mount("/", routes![generate_mnemonic,get_private_key,get_public_key]).launch();

    let controller = FrontController::new();
    rocket::ignite().manage(controller).mount("/", routes![generate_mnemonic,get_private_key,get_public_key]).launch();
}












// code lib
/*
	 fn on_request(&self, request: &mut Request, _: &Data) {
        if request.method() == Method::Get {
            self.get.fetch_add(1, Ordering::Relaxed);
        } else if request.method() == Method::Post {
            self.post.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn on_response(&self, request: &Request, response: &mut Response) {
        // Don't change a successful user's response, ever.
        if response.status() != Status::NotFound {
            return
        }

        if request.method() == Method::Get && request.uri().path() == "/generate_mnemonic" {

            let body = format!("{}", self.get_mnemonic());
            response.set_status(Status::Ok);
            response.set_header(ContentType::Plain);
            response.set_sized_body(Cursor::new(body));
        }

        if request.method() == Method::Get && request.uri().path() == "/get_private_key" {

            let body = format!("{}", self.get_publick_key());
            response.set_status(Status::Ok);
            response.set_header(ContentType::Plain);
            response.set_sized_body(Cursor::new(body));
        }

    }*/

/*
fn get_libra_wallet(mnemonic_file:Option<String>) -> Result<WalletLibrary> {
    let wallet_recovery_file_path = if let Some(input_mnemonic_word) = mnemonic_file {
        Path::new(&input_mnemonic_word).to_path_buf()
    } else {
        let mut file_path = std::env::current_dir()?;
        file_path.push(CLIENT_WALLET_MNEMONIC_FILE);
        file_path
    };

    let wallet = if let OK(recoverd_wallet) = io_utils::recover(&wallet_recovery_file_path) {
        recoverd_wallet
    } else {
        let new_wallet = WalletLibrary::new();
        new_wallet.write_recovery(&wallet_recovery_file_path)?;
        new_wallet
    };
    OK(wallet)
}
*/
