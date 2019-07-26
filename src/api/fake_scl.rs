// Copyright 2019 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

use super::safe_net::AppendOnlyDataRawData;
use super::xorurl::create_random_xorname;
use super::{Error, ResultReturn, SafeApp, XorUrlEncoder};
use crate::api::helpers::{parse_hex, vec_to_hex, xorname_from_pk, xorname_to_hex};
use log::debug;
use safe_nd::{MDataValue, PublicKey as SafeNdPublicKey, SeqMutableData, XorName};
use safecoin::{Coins, NanoCoins};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::convert::TryFrom;
use std::io::Write;
use std::str::FromStr;
use std::{fs, str};
use threshold_crypto::{PublicKey, SecretKey};

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
struct CoinBalance {
    owner: PublicKey,
    value: String,
}

type AppendOnlyDataMock = Vec<(Vec<u8>, Vec<u8>)>;
type TxStatusList = BTreeMap<String, String>;
type XorNameStr = String;
type SeqMutableDataMock = BTreeMap<String, MDataValue>;

const FAKE_VAULT_FILE: &str = "./fake_vault_data.txt";

#[derive(Default, Serialize, Deserialize)]
struct MockData {
    coin_balances: BTreeMap<XorNameStr, CoinBalance>,
    txs: BTreeMap<XorNameStr, TxStatusList>, // keep track of TX status per tx ID, per xorname
    published_seq_append_only: BTreeMap<XorNameStr, AppendOnlyDataMock>, // keep a versioned map of data per xorname
    mutable_data: BTreeMap<XorNameStr, SeqMutableDataMock>,
    published_immutable_data: BTreeMap<XorNameStr, Vec<u8>>,
}

#[derive(Default)]
pub struct SafeAppFake {
    mock_data: MockData,
}

/// Writes the mock data onto the mock file
impl Drop for SafeAppFake {
    fn drop(&mut self) {
        let serialised = serde_json::to_string(&self.mock_data)
            .expect("Failed to serialised fake vault data to write on file");
        debug!("serialised = {}", serialised);

        let mut file =
            fs::File::create(&FAKE_VAULT_FILE).expect("Failed to create fake vault DB file");
        let _ = file
            .write(serialised.as_bytes())
            .expect("Failed to write fake vault DB file");
    }
}

impl SafeAppFake {
    // private helper
    fn get_balance_from_xorname(&self, xorname: &XorName) -> ResultReturn<String> {
        match self.mock_data.coin_balances.get(&xorname_to_hex(&xorname)) {
            None => Err(Error::ContentNotFound(
                "CoinBalance data not found".to_string(),
            )),
            Some(coin_balance) => {
                let balance_str = coin_balance
                    .value
                    .to_string()
                    .replace("Coins(", "")
                    .replace(")", "");
                let balance = balance_str.parse::<f64>().map_err(|e| {
                    Error::Unexpected(format!("Failed to format balance output: {}", e))
                })?;
                Ok(format!("{:.*}", 9, balance))
            }
        }
    }

    fn fetch_pk_from_xorname(&self, xorname: &XorName) -> ResultReturn<PublicKey> {
        match self.mock_data.coin_balances.get(&xorname_to_hex(&xorname)) {
            None => Err(Error::ContentNotFound(
                "CoinBalance data not found".to_string(),
            )),
            Some(coin_balance) => Ok(coin_balance.owner),
        }
    }
}

impl SafeApp for SafeAppFake {
    fn new() -> SafeAppFake {
        let mock_data = match fs::File::open(&FAKE_VAULT_FILE) {
            Ok(file) => {
                let deserialised: MockData =
                    serde_json::from_reader(&file).expect("Failed to read fake vault DB file");
                deserialised
            }
            Err(error) => {
                debug!("Error reading mock file. {}", error.to_string());
                MockData::default()
            }
        };

        SafeAppFake { mock_data }
    }

    fn connect(&mut self, _app_id: &str, _auth_credentials: Option<&str>) -> ResultReturn<()> {
        debug!("Using mock so there is no connection to network");
        Ok(())
    }

    fn create_balance(
        &mut self,
        from_sk: Option<SecretKey>,
        new_balance_owner: PublicKey,
        amount: &str,
    ) -> ResultReturn<XorName> {
        match from_sk {
            Some(sk) => {
                let from_pk = sk.public_key();
                let from_xorname = xorname_from_pk(&from_pk);
                let from_balance = Coins::from_str(&self.get_balance_from_sk(sk)?)?;
                let from_nano_balance = NanoCoins::try_from(from_balance)?;
                let amount_coin = Coins::from_str(amount)?;
                let amount_nano = NanoCoins::try_from(amount_coin)?;
                if from_nano_balance.num() < amount_nano.num() {
                    return Err(Error::NotEnoughBalance(from_nano_balance.num().to_string()));
                }
                let from_new_amount = NanoCoins::new(from_nano_balance.num() - amount_nano.num())?;
                self.mock_data.coin_balances.insert(
                    xorname_to_hex(&from_xorname),
                    CoinBalance {
                        owner: from_pk,
                        value: Coins::from(from_new_amount).to_string(),
                    },
                );
            }
            None => {
                /* TODO: we should have a default wallet and substract from there */
                return Err(Error::NetDataError(
                    "Failed to create a CoinBalance: \"NoSuchBalance\"".to_string(),
                ));
            }
        };

        let to_xorname = xorname_from_pk(&new_balance_owner);
        self.mock_data.coin_balances.insert(
            xorname_to_hex(&to_xorname),
            CoinBalance {
                owner: new_balance_owner,
                value: amount.to_string(),
            },
        );

        Ok(to_xorname)
    }

    fn allocate_test_coins(&mut self, to_pk: PublicKey, amount: &str) -> ResultReturn<XorName> {
        let xorname = xorname_from_pk(&to_pk);
        self.mock_data.coin_balances.insert(
            xorname_to_hex(&xorname),
            CoinBalance {
                owner: (to_pk),
                value: amount.to_string(),
            },
        );

        Ok(xorname)
    }

    fn get_balance_from_sk(&self, sk: SecretKey) -> ResultReturn<String> {
        let pk = sk.public_key();
        let xorname = xorname_from_pk(&pk);
        self.get_balance_from_xorname(&xorname)
    }

    fn safecoin_transfer_to_xorname(
        &mut self,
        from_sk: SecretKey,
        to_xorname: XorName,
        tx_id: u64,
        amount: &str,
    ) -> ResultReturn<u64> {
        let to_xorname_hex = xorname_to_hex(&to_xorname);
        let from_pk = from_sk.public_key();
        let from_xorname = xorname_from_pk(&from_pk);

        // generate TX in destination section (to_pk)
        let mut txs_for_xorname = match self.mock_data.txs.get(&to_xorname_hex) {
            Some(txs) => txs.clone(),
            None => BTreeMap::new(),
        };
        txs_for_xorname.insert(
            tx_id.to_string(),
            format!("Success({})", amount).to_string(),
        );
        self.mock_data
            .txs
            .insert(to_xorname_hex.clone(), txs_for_xorname);

        let amount_coin =
            (Coins::from_str(amount)).map_err(|err| Error::InvalidAmount(format!("{:?}", err)))?;

        // reduce balance from safecoin_transferer
        let from_balance = Coins::from_str(&self.get_balance_from_sk(from_sk.clone())?)?;
        let from_nano_balance = NanoCoins::try_from(from_balance)?;
        let amount_nano = NanoCoins::try_from(amount_coin)?;
        if from_nano_balance.num() < amount_nano.num() {
            return Err(Error::NotEnoughBalance(from_nano_balance.num().to_string()));
        }
        let from_new_amount = NanoCoins::new(from_nano_balance.num() - amount_nano.num())?;
        self.mock_data.coin_balances.insert(
            xorname_to_hex(&from_xorname),
            CoinBalance {
                owner: from_pk,
                value: Coins::from(from_new_amount).to_string(),
            },
        );

        // credit destination
        let to_balance = Coins::from_str(&self.get_balance_from_xorname(&to_xorname)?)?;
        let to_pk = self.fetch_pk_from_xorname(&to_xorname)?;
        let to_nano_balance = NanoCoins::try_from(to_balance)?;
        let to_new_amount = NanoCoins::new(to_nano_balance.num() + amount_nano.num())?;
        self.mock_data.coin_balances.insert(
            to_xorname_hex,
            CoinBalance {
                owner: to_pk,
                value: Coins::from(to_new_amount).to_string(),
            },
        );

        Ok(tx_id)
    }

    fn safecoin_transfer_to_pk(
        &mut self,
        from_sk: SecretKey,
        to_pk: PublicKey,
        tx_id: u64,
        amount: &str,
    ) -> ResultReturn<u64> {
        let to_xorname = xorname_from_pk(&to_pk);
        self.safecoin_transfer_to_xorname(from_sk, to_xorname, tx_id, amount)
    }

    fn get_transaction(&self, tx_id: u64, pk: PublicKey, _sk: SecretKey) -> ResultReturn<String> {
        let xorname = xorname_from_pk(&pk);
        let txs_for_xorname = &self.mock_data.txs[&xorname_to_hex(&xorname)];
        let tx_state = txs_for_xorname.get(&tx_id.to_string()).ok_or_else(|| {
            Error::ContentNotFound(format!("Transaction not found with id '{}'", tx_id))
        })?;
        Ok(tx_state.to_string())
    }

    fn files_put_published_immutable(&mut self, data: &[u8]) -> ResultReturn<XorName> {
        let xorname = create_random_xorname();
        // TODO: hash to get xorname.
        self.mock_data
            .published_immutable_data
            .insert(xorname_to_hex(&xorname), data.to_vec());

        Ok(xorname)
    }

    fn files_get_published_immutable(&self, xorname: XorName) -> ResultReturn<Vec<u8>> {
        let data = match self
            .mock_data
            .published_immutable_data
            .get(&xorname_to_hex(&xorname))
        {
            Some(data) => data.clone(),
            None => {
                return Err(Error::NetDataError(
                    "No ImmutableData found at this address".to_string(),
                ))
            }
        };

        Ok(data)
    }

    fn put_seq_append_only_data(
        &mut self,
        data: Vec<(Vec<u8>, Vec<u8>)>,
        name: Option<XorName>,
        _tag: u64,
        _permissions: Option<String>,
    ) -> ResultReturn<XorName> {
        let xorname = name.unwrap_or_else(create_random_xorname);

        self.mock_data
            .published_seq_append_only
            .insert(xorname_to_hex(&xorname), data);

        Ok(xorname)
    }

    fn append_seq_append_only_data(
        &mut self,
        data: Vec<(Vec<u8>, Vec<u8>)>,
        _new_version: u64,
        name: XorName,
        _tag: u64,
    ) -> ResultReturn<u64> {
        let xorname_hex = xorname_to_hex(&name);
        let mut seq_append_only = match self.mock_data.published_seq_append_only.get(&xorname_hex) {
            Some(seq_append_only) => seq_append_only.clone(),
            None => {
                return Err(Error::ContentNotFound(format!(
                    "Sequential AppendOnlyData not found at Xor name: {}",
                    xorname_hex
                )))
            }
        };

        seq_append_only.extend(data);
        self.mock_data
            .published_seq_append_only
            .insert(xorname_hex, seq_append_only.to_vec());

        Ok(seq_append_only.len() as u64)
    }

    fn get_latest_seq_append_only_data(
        &self,
        name: XorName,
        _tag: u64,
    ) -> ResultReturn<(u64, AppendOnlyDataRawData)> {
        let xorname_hex = xorname_to_hex(&name);
        debug!("Attempting to locate scl mock mdata: {}", xorname_hex);

        match self.mock_data.published_seq_append_only.get(&xorname_hex) {
            Some(seq_append_only) => {
                let latest_index = seq_append_only.len() - 1;
                let last_entry = seq_append_only.get(latest_index).ok_or_else(|| {
                    Error::EmptyContent(format!(
                        "Empty Sequential AppendOnlyData found at Xor name {}",
                        xorname_hex
                    ))
                })?;
                Ok((seq_append_only.len() as u64, last_entry.clone()))
            }
            None => Err(Error::ContentNotFound(format!(
                "Sequential AppendOnlyData not found at Xor name: {}",
                xorname_hex
            ))),
        }
    }

    #[allow(dead_code)]
    fn get_current_seq_append_only_data_version(
        &self,
        name: XorName,
        _tag: u64,
    ) -> ResultReturn<u64> {
        debug!("Getting seq appendable data, length for: {:?}", name);

        let xorname_hex = xorname_to_hex(&name);

        let length = match self.mock_data.published_seq_append_only.get(&xorname_hex) {
            Some(seq_append_only) => seq_append_only.len(),
            None => {
                return Err(Error::ContentNotFound(format!(
                    "Sequential AppendOnlyData not found at Xor name: {}",
                    xorname_hex
                )))
            }
        };

        // return the version
        Ok(length as u64)
    }

    // TODO: add impl
    fn get_seq_append_only_data(
        &self,
        _name: XorName,
        _tag: u64,
        _version: u64,
    ) -> ResultReturn<AppendOnlyDataRawData> {
        Ok(AppendOnlyDataRawData::default())
    }

    fn put_seq_mutable_data(
        &mut self,
        name: Option<XorName>,
        _tag: u64,
        // _data: Option<String>,
        _permissions: Option<String>,
    ) -> ResultReturn<XorName> {
        let xorname = name.unwrap_or_else(create_random_xorname);
        let seq_md = match self.mock_data.mutable_data.get(&xorname_to_hex(&xorname)) {
            Some(uao) => uao.clone(),
            None => BTreeMap::new(),
        };

        self.mock_data
            .mutable_data
            .insert(xorname_to_hex(&xorname), seq_md);

        Ok(xorname)
    }

    fn get_seq_mdata(&self, xorname: XorName, tag: u64) -> ResultReturn<SeqMutableData> {
        let xorname_hex = xorname_to_hex(&xorname);
        debug!("attempting to locate scl mock mdata: {}", xorname_hex);

        match self.mock_data.mutable_data.get(&xorname_hex) {
            Some(seq_md) => {
                let mut seq_md_with_vec: BTreeMap<Vec<u8>, MDataValue> = BTreeMap::new();
                seq_md.iter().for_each(|(k, v)| {
                    seq_md_with_vec.insert(parse_hex(k), v.clone());
                });

                Ok(SeqMutableData::new_with_data(
                    xorname,
                    tag,
                    seq_md_with_vec,
                    BTreeMap::default(),
                    SafeNdPublicKey::Bls(SecretKey::random().public_key()),
                ))
            }
            None => Err(Error::ContentNotFound(format!(
                "Sequential AppendOnlyData not found at Xor name: {}",
                xorname_hex
            ))),
        }
    }

    fn seq_mutable_data_insert(
        &mut self,
        xorurl: &str,
        tag: u64,
        key: Vec<u8>,
        value: &[u8],
    ) -> ResultReturn<()> {
        let xorurl_encoder = XorUrlEncoder::from_url(xorurl)?;
        let xorname = xorurl_encoder.xorname();
        let seq_md = self.get_seq_mdata(xorname, tag)?;
        let mut data = seq_md.entries().clone();

        data.insert(
            key.to_vec(),
            MDataValue {
                data: value.to_vec(),
                version: 0,
            },
        );

        let mut seq_md_with_str: BTreeMap<String, MDataValue> = BTreeMap::new();
        data.iter().for_each(|(k, v)| {
            seq_md_with_str.insert(vec_to_hex(k.to_vec()), v.clone());
        });
        self.mock_data
            .mutable_data
            .insert(xorname_to_hex(&xorname), seq_md_with_str);

        Ok(())
    }

    fn mutable_data_delete(
        &mut self,
        _xorname: &XorName,
        _tag: u64,
        _key: &[u8],
    ) -> ResultReturn<()> {
        Ok(())
    }

    fn seq_mutable_data_get_value(
        &mut self,
        xorurl: &str,
        tag: u64,
        key: Vec<u8>,
    ) -> ResultReturn<MDataValue> {
        let xorurl_encoder = XorUrlEncoder::from_url(xorurl)?;
        let xorname = xorurl_encoder.xorname();
        let seq_md = self.get_seq_mdata(xorname, tag)?;
        match seq_md.get(&key.to_vec()) {
            Some(value) => Ok(value.clone()),
            None => Err(Error::EntryNotFound(format!(
                "Entry not found in Sequential MutableData found at Xor name: {}",
                xorname_to_hex(&xorname)
            ))),
        }
    }

    fn list_seq_mdata_entries(
        &self,
        xorurl: &str,
        tag: u64,
    ) -> ResultReturn<BTreeMap<Vec<u8>, MDataValue>> {
        debug!("Listing seq_mdata_entries for: {}", xorurl);
        let xorurl_encoder = XorUrlEncoder::from_url(xorurl)?;
        let seq_md = self.get_seq_mdata(xorurl_encoder.xorname(), tag)?;

        let mut res = BTreeMap::new();
        seq_md.entries().iter().for_each(|elem| {
            res.insert(elem.0.clone(), elem.1.clone());
        });

        Ok(res)
    }

    //TODO: Replace with real mock code
    #[allow(dead_code)]
    fn seq_mutable_data_update(
        &self,
        _xorurl: &str,
        _type_tag: u64,
        _key: &[u8],
        _value: &[u8],
        _version: u64,
    ) -> ResultReturn<()> {
        Ok(())
    }
}

// Unit tests

#[test]
fn test_allocate_test_coins() {
    use self::SafeApp;
    use threshold_crypto::SecretKey;
    use unwrap::unwrap;

    let mut mock = SafeAppFake::new();

    let sk_to = SecretKey::random();
    let pk_to = sk_to.public_key();

    let balance = "2.345678912";
    unwrap!(mock.allocate_test_coins(pk_to, balance));
    let current_balance = unwrap!(mock.get_balance_from_sk(sk_to));
    println!("Current balance: {}", current_balance);
    assert_eq!(balance, &current_balance);
}

#[test]
fn test_create_balance() {
    use self::SafeApp;
    use threshold_crypto::SecretKey;
    use unwrap::unwrap;

    let mut mock = SafeAppFake::new();

    let sk = SecretKey::random();
    let pk = sk.public_key();

    let balance = "2.345678912";
    unwrap!(mock.allocate_test_coins(pk, balance));

    let sk_to = SecretKey::random();
    let pk_to = sk_to.public_key();
    println!(
        "New CoinBalance at: {:?}",
        mock.create_balance(Some(sk), pk_to, "1.234567891")
    );
}

#[test]
fn test_check_balance() {
    use self::SafeApp;
    use threshold_crypto::SecretKey;
    use unwrap::unwrap;

    let mut mock = SafeAppFake::new();

    let sk = SecretKey::random();
    let pk = sk.public_key();

    let balance = "2.300000000";
    unwrap!(mock.allocate_test_coins(pk, balance));
    let current_balance = unwrap!(mock.get_balance_from_sk(sk.clone()));
    println!("Current balance: {}", current_balance);
    assert_eq!(balance, &current_balance);

    let sk_to = SecretKey::random();
    let pk_to = sk_to.public_key();
    let preload = "1.234567891";
    println!(
        "New CoinBalance at: {:?}",
        mock.create_balance(Some(sk.clone()), pk_to, preload)
    );
    let current_balance = unwrap!(mock.get_balance_from_sk(sk_to));
    println!("Current balance: {}", current_balance);
    assert_eq!(preload, &current_balance);

    let current_balance = unwrap!(mock.get_balance_from_sk(sk));
    println!("Current balance: {}", current_balance);
    assert_eq!(
        "1.065432109", /* == 2.3 - 1.234567891*/
        &current_balance
    );
}

#[test]
fn test_safecoin_transfer() {
    use self::SafeApp;
    use rand_core::RngCore;
    use threshold_crypto::SecretKey;
    use unwrap::unwrap;

    let mut mock = SafeAppFake::new();

    let sk1 = SecretKey::random();
    let pk1 = sk1.public_key();

    let sk2 = SecretKey::random();
    let pk2 = sk2.public_key();

    let balance1 = "2.500000000";
    let balance2 = "5.700000000";
    println!(
        "Allocate testcoins in new CoinBalance 1 at: {:?}",
        mock.allocate_test_coins(pk1, balance1)
    );

    println!(
        "Allocate testcoins in new CoinBalance 2 at: {:?}",
        mock.allocate_test_coins(pk2, balance2)
    );

    let curr_balance1 = unwrap!(mock.get_balance_from_sk(sk1.clone()));
    let curr_balance2 = unwrap!(mock.get_balance_from_sk(sk2.clone()));
    println!(
        "Current balances before TX: {} and {}",
        curr_balance1, curr_balance2
    );

    assert_eq!(balance1, curr_balance1);
    assert_eq!(balance2, curr_balance2);

    let mut rng = rand::thread_rng();
    let tx_id = rng.next_u64();
    println!("UUID {}", tx_id);

    let _ = unwrap!(mock.safecoin_transfer_to_xorname(
        sk1.clone(),
        xorname_from_pk(&pk2),
        tx_id,
        "1.4"
    ));
    println!(
        "Current TX state: {}",
        unwrap!(mock.get_transaction(tx_id, pk2, sk2.clone()))
    );

    let curr_balance1 = unwrap!(mock.get_balance_from_sk(sk1));
    let curr_balance2 = unwrap!(mock.get_balance_from_sk(sk2));
    println!(
        "Current balances after TX: {} and {}",
        curr_balance1, curr_balance2
    );

    assert_eq!(curr_balance1, "1.100000000");
    assert_eq!(curr_balance2, "7.100000000");
}
