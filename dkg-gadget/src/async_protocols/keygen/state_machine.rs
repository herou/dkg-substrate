// Copyright 2022 Webb Technologies Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::async_protocols::{
	blockchain_interface::BlockchainInterface, get_party_session_id,
	state_machine::StateMachineHandler, AsyncProtocolParameters, ProtocolType,
};
use async_trait::async_trait;
use dkg_primitives::types::{DKGError, DKGMessage, DKGMsgPayload, DKGPublicKeyMessage};
use dkg_runtime_primitives::crypto::Public;
use futures::channel::mpsc::UnboundedSender;
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::keygen::{
	Keygen, ProtocolMessage,
};
use round_based::{Msg, StateMachine};

#[async_trait]
impl StateMachineHandler for Keygen {
	type AdditionalReturnParam = ();
	type Return = <Self as StateMachine>::Output;

	fn handle_unsigned_message(
		to_async_proto: &UnboundedSender<Msg<ProtocolMessage>>,
		msg: Msg<DKGMessage<Public>>,
		local_ty: &ProtocolType,
	) -> Result<(), <Self as StateMachine>::Err> {
		let DKGMessage { payload, session_id, .. } = msg.body;
		// Send the payload to the appropriate AsyncProtocols
		match payload {
			DKGMsgPayload::Keygen(msg) => {
				log::info!(target: "dkg_gadget::async_protocol::keygen", "Handling Keygen inbound message from id={}, session={}", msg.sender_id, session_id);
				let message: Msg<ProtocolMessage> =
					match serde_json::from_slice(msg.keygen_msg.as_slice()) {
						Ok(message) => message,
						Err(err) => {
							log::error!(target: "dkg_gadget::async_protocol::keygen", "Error deserializing message: {}", err);
							// Skip this message.
							return Ok(())
						},
					};

				if let Some(recv) = message.receiver.as_ref() {
					if *recv != local_ty.get_i() {
						log::info!("Skipping passing of message to async proto since not intended for local");
						return Ok(())
					}
				}
				if let Err(e) = to_async_proto.unbounded_send(message) {
					log::error!(target: "dkg_gadget::async_protocol::keygen", "Error sending message to async proto: {}", e);
				}
			},

			err =>
				log::debug!(target: "dkg_gadget::async_protocol::keygen", "Invalid payload received: {:?}", err),
		}

		Ok(())
	}

	async fn on_finish<BI: BlockchainInterface + 'static>(
		local_key: <Self as StateMachine>::Output,
		params: AsyncProtocolParameters<BI>,
		_: Self::AdditionalReturnParam,
		_: u8,
	) -> Result<<Self as StateMachine>::Output, DKGError> {
		log::info!(target: "dkg_gadget::async_protocol::keygen", "Completed keygen stage successfully!");
		// PublicKeyGossip (we need meta handler to handle this)
		// when keygen finishes, we gossip the signed key to peers.
		// [1] create the message, call the "public key gossip" in
		// public_key_gossip.rs:gossip_public_key [2] store public key locally (public_keys.rs:
		// store_aggregated_public_keys)
		let session_id = get_party_session_id(&params).1;
		let pub_key_msg = DKGPublicKeyMessage {
			session_id,
			pub_key: local_key.public_key().to_bytes(true).to_vec(),
			signature: vec![],
		};

		params.engine.gossip_public_key(pub_key_msg)?;
		params.engine.store_public_key(local_key.clone(), session_id)?;

		Ok(local_key)
	}
}
