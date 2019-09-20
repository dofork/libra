<a href="https://developers.libra.org">
	<img width="200" src="./.assets/libra.png" alt="Libra Logo" />
</a>

---
### Purpose of this project
The original wallet in libra is a stdin/stdout app. It's inconvenience to interact with this wallet.
So this project aim to provide RESTful API for the wallet which you could interact through web.
### RESTful API
. create account
```
http://Host:port/create_account
```
. mint for a address
```
http://Host:port/mint/<receiver_address>/<num_coins>
```
. query balance for a address
```
http://Host:port/get_balance/<address>
```
. transfer coins from sender to receiver
```
http://Host:port/transfer_coins/<sender_address>/<receiver_address>/<coins>/<gas_unit_price>/<max_gas>
```
. recovery wallet from a mnemonic string
```
http://Host:port/recovery_wallet/<mnemonic>
```
### Build the project

Step1: Get the code
```
git clone https://github.com/dofork/libra.git
git checkout testnet
```

Step2: Setup the libra environment
```
./scripts/dev_setup.sh
```

Step3: Build the restful wallet
```
cd libra/rocket/examples/libra
cargo run
```

The restful wallet will launched from http://localhost:8000

Connect me if you have any question.
Mail:147796352@qq.com
WeChat:18682186902

