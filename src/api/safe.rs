// Copyright 2019 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

#[cfg(not(feature = "scl-mock"))]
use crate::api::safe_client_libs::SafeApp;
use log::{debug, info};

#[cfg(feature = "scl-mock")]
use crate::api::scl_mock::SafeApp;

pub struct Safe {
    pub safe_app: SafeApp,
    pub xorurl_base: String,
}
