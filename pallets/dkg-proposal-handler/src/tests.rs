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
//
use crate::{mock::*, UnsignedProposalQueue};
use codec::Encode;
use frame_support::{
	assert_err, assert_ok,
	traits::{Hooks, OnFinalize},
	weights::{constants::RocksDbWeight, DispatchClass},
};
use sp_runtime::offchain::storage::{StorageRetrievalError, StorageValueRef};
use sp_std::vec::Vec;

use super::mock::DKGProposalHandler;
use dkg_runtime_primitives::{
	offchain::storage_keys::OFFCHAIN_SIGNED_PROPOSALS, DKGPayloadKey, OffchainSignedProposals,
	ProposalAction, ProposalHandlerTrait, ProposalHeader, TransactionV2, TypedChainId,
};
use sp_core::sr25519;
use sp_runtime::offchain::storage::MutateStorageError;
use webb_proposals::{Proposal, ProposalKind};

// *** Utility ***

fn add_proposal_to_offchain_storage(prop: Proposal) {
	let proposals_ref = StorageValueRef::persistent(OFFCHAIN_SIGNED_PROPOSALS);

	let update_res: Result<OffchainSignedProposals<u64>, MutateStorageError<_, ()>> = proposals_ref
		.mutate(
			|val: Result<Option<OffchainSignedProposals<u64>>, StorageRetrievalError>| match val {
				Ok(Some(mut ser_props)) => {
					ser_props.proposals.push((vec![prop], 0));
					Ok(ser_props)
				},
				_ => {
					let mut prop_wrapper = OffchainSignedProposals::<u64>::default();
					prop_wrapper.proposals.push((vec![prop], 0));
					Ok(prop_wrapper)
				},
			},
		);

	assert_ok!(update_res);
}

fn check_offchain_proposals_num_eq(num: usize) {
	let proposals_ref = StorageValueRef::persistent(OFFCHAIN_SIGNED_PROPOSALS);
	let stored_props: Option<OffchainSignedProposals<u64>> =
		proposals_ref.get::<OffchainSignedProposals<u64>>().unwrap();
	assert!(stored_props.is_some(), "{}", true);

	assert_eq!(stored_props.unwrap().proposals.len(), num);
}

// helper function to skip blocks
pub fn run_n_blocks(n: u64) -> u64 {
	// lets leave enough weight to read a queue with length one and remove one item
	let idle_weight = RocksDbWeight::get().reads_writes(1, 1);
	let start_block = System::block_number();

	for block_number in start_block..=n {
		System::set_block_number(block_number);

		// ensure the on_idle is executed
		<frame_system::Pallet<Test>>::register_extra_weight_unchecked(
			DKGProposalHandler::on_idle(block_number, idle_weight),
			DispatchClass::Mandatory,
		);

		<frame_system::Pallet<Test> as OnFinalize<u64>>::on_finalize(block_number);
	}

	System::block_number()
}

// *** Tests ***

#[test]
fn handle_empty_proposal() {
	execute_test_with(|| {
		let prop: Vec<u8> = Vec::new();

		assert_err!(
			DKGProposalHandler::handle_unsigned_proposal(prop, ProposalAction::Sign(0)),
			crate::Error::<Test>::ProposalFormatInvalid
		);

		assert_eq!(DKGProposalHandler::get_unsigned_proposals().len(), 0);
	});
}

#[test]
fn handle_unsigned_eip2930_transaction_proposal_success() {
	execute_test_with(|| {
		let tx_v_2 = TransactionV2::EIP2930(mock_eth_tx_eip2930(0));

		assert_ok!(DKGProposalHandler::force_submit_unsigned_proposal(
			RuntimeOrigin::root(),
			Proposal::Unsigned { kind: ProposalKind::EVM, data: tx_v_2.encode() },
		));

		assert_eq!(DKGProposalHandler::get_unsigned_proposals().len(), 1);

		assert!(
			DKGProposalHandler::unsigned_proposals(
				TypedChainId::Evm(0),
				DKGPayloadKey::EVMProposal(0.into())
			)
			.is_some(),
			"{}",
			true
		);
	})
}

#[test]
fn handle_anchor_update_proposal_success() {
	execute_test_with(|| {
		let proposal_raw: [u8; 104] = [
			0, 0, 0, 0, 0, 0, 223, 22, 158, 136, 193, 21, 177, 236, 107, 47, 234, 158, 193, 108,
			153, 64, 171, 132, 14, 7, 1, 0, 0, 0, 5, 57, 68, 52, 123, 169, 0, 0, 0, 1, 1, 0, 0, 0,
			122, 105, 0, 0, 0, 0, 37, 168, 34, 127, 179, 164, 10, 49, 149, 165, 172, 173, 194, 178,
			181, 131, 238, 94, 88, 214, 203, 31, 58, 98, 176, 16, 209, 39, 221, 166, 75, 249, 181,
			131, 238, 94, 88, 214, 203, 31, 58, 98, 176, 16, 209, 39, 221, 166, 75, 249, 181, 131,
			238, 94,
		];

		assert_ok!(DKGProposalHandler::handle_unsigned_proposal(
			proposal_raw.to_vec(),
			ProposalAction::Sign(0)
		));

		assert_eq!(DKGProposalHandler::get_unsigned_proposals().len(), 1);
	})
}

#[test]
fn should_handle_proposer_set_update_proposal_success() {
	execute_test_with(|| {
		let proposal_raw: [u8; 48] = [
			1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 1, 2, 3, 4, 5, 6, 7, 8,
			9, 10, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 1, 2, 3, 4, 0, 0, 0, 1,
		];

		assert_ok!(DKGProposalHandler::handle_unsigned_proposer_set_update_proposal(
			proposal_raw.to_vec(),
			ProposalAction::Sign(0)
		));

		assert_eq!(DKGProposalHandler::get_unsigned_proposals().len(), 1);
	})
}

#[test]
fn store_signed_proposal_offchain() {
	execute_test_with(|| {
		let tx_v_2 = TransactionV2::EIP2930(mock_eth_tx_eip2930(0));

		assert_ok!(DKGProposalHandler::force_submit_unsigned_proposal(
			RuntimeOrigin::root(),
			Proposal::Unsigned { kind: ProposalKind::EVM, data: tx_v_2.encode() },
		));

		assert!(
			DKGProposalHandler::unsigned_proposals(
				TypedChainId::Evm(0),
				DKGPayloadKey::EVMProposal(0.into())
			)
			.is_some(),
			"{}",
			true
		);

		let signed_proposal = mock_signed_proposal(tx_v_2);

		add_proposal_to_offchain_storage(signed_proposal);

		check_offchain_proposals_num_eq(1);
	})
}

#[test]
fn submit_signed_proposal_onchain_success() {
	execute_test_with(|| {
		let tx_v_2 = TransactionV2::EIP2930(mock_eth_tx_eip2930(0));

		assert_ok!(DKGProposalHandler::force_submit_unsigned_proposal(
			RuntimeOrigin::root(),
			Proposal::Unsigned { kind: ProposalKind::EVM, data: tx_v_2.encode() },
		));

		assert!(
			DKGProposalHandler::unsigned_proposals(
				TypedChainId::Evm(0),
				DKGPayloadKey::EVMProposal(0.into())
			)
			.is_some(),
			"{}",
			true
		);

		let signed_proposal = mock_signed_proposal(tx_v_2);

		add_proposal_to_offchain_storage(signed_proposal);

		assert_ok!(DKGProposalHandler::submit_signed_proposal_onchain(0));

		check_offchain_proposals_num_eq(0);
	});
}

#[test]
fn submit_signed_proposal_success() {
	execute_test_with(|| {
		let tx_v_2 = TransactionV2::EIP2930(mock_eth_tx_eip2930(0));

		assert_ok!(DKGProposalHandler::force_submit_unsigned_proposal(
			RuntimeOrigin::root(),
			Proposal::Unsigned { kind: ProposalKind::EVM, data: tx_v_2.encode() },
		));

		assert!(
			DKGProposalHandler::unsigned_proposals(
				TypedChainId::Evm(0),
				DKGPayloadKey::EVMProposal(0.into())
			)
			.is_some(),
			"{}",
			true
		);

		let signed_proposal = mock_signed_proposal(tx_v_2);

		assert_ok!(DKGProposalHandler::submit_signed_proposals(
			RuntimeOrigin::signed(sr25519::Public::from_raw([1; 32])),
			vec![signed_proposal]
		));

		assert!(
			DKGProposalHandler::unsigned_proposals(
				TypedChainId::Evm(0),
				DKGPayloadKey::EVMProposal(0.into())
			)
			.is_none(),
			"{}",
			true
		);

		assert!(
			DKGProposalHandler::signed_proposals(
				TypedChainId::Evm(0),
				DKGPayloadKey::EVMProposal(0.into())
			)
			.is_some(),
			"{}",
			true
		);
	});
}

#[test]
fn submit_signed_proposal_already_exists() {
	execute_test_with(|| {
		// First submission
		let tx_v_2 = TransactionV2::EIP2930(mock_eth_tx_eip2930(0));

		assert_ok!(DKGProposalHandler::force_submit_unsigned_proposal(
			RuntimeOrigin::root(),
			Proposal::Unsigned { kind: ProposalKind::EVM, data: tx_v_2.encode() },
		));

		assert!(
			DKGProposalHandler::unsigned_proposals(
				TypedChainId::Evm(0),
				DKGPayloadKey::EVMProposal(0.into())
			)
			.is_some(),
			"{}",
			true
		);

		let signed_proposal = mock_signed_proposal(tx_v_2.clone());

		assert_ok!(DKGProposalHandler::submit_signed_proposals(
			RuntimeOrigin::signed(sr25519::Public::from_raw([1; 32])),
			vec![signed_proposal.clone()]
		));

		assert!(
			DKGProposalHandler::unsigned_proposals(
				TypedChainId::Evm(0),
				DKGPayloadKey::EVMProposal(0.into())
			)
			.is_none(),
			"{}",
			true
		);

		assert!(
			DKGProposalHandler::signed_proposals(
				TypedChainId::Evm(0),
				DKGPayloadKey::EVMProposal(0.into())
			)
			.is_some(),
			"{}",
			true
		);

		// Second submission
		assert_ok!(DKGProposalHandler::force_submit_unsigned_proposal(
			RuntimeOrigin::root(),
			Proposal::Unsigned { kind: ProposalKind::EVM, data: tx_v_2.encode() },
		));

		assert!(
			DKGProposalHandler::unsigned_proposals(
				TypedChainId::Evm(0),
				DKGPayloadKey::EVMProposal(0.into())
			)
			.is_some(),
			"{}",
			true
		);

		assert_ok!(DKGProposalHandler::submit_signed_proposals(
			RuntimeOrigin::signed(sr25519::Public::from_raw([1; 32])),
			vec![signed_proposal]
		));

		assert!(
			DKGProposalHandler::unsigned_proposals(
				TypedChainId::Evm(0),
				DKGPayloadKey::EVMProposal(0.into())
			)
			.is_none(),
			"{}",
			true
		);
		assert!(
			DKGProposalHandler::signed_proposals(
				TypedChainId::Evm(0),
				DKGPayloadKey::EVMProposal(0.into())
			)
			.is_some(),
			"{}",
			true
		);
	});
}

#[test]
fn submit_signed_proposal_fail_invalid_sig() {
	execute_test_with(|| {
		let tx_v_2 = TransactionV2::EIP2930(mock_eth_tx_eip2930(0));

		assert_ok!(DKGProposalHandler::force_submit_unsigned_proposal(
			RuntimeOrigin::root(),
			Proposal::Unsigned { kind: ProposalKind::EVM, data: tx_v_2.encode() },
		));

		assert!(
			DKGProposalHandler::unsigned_proposals(
				TypedChainId::Evm(0),
				DKGPayloadKey::EVMProposal(0.into())
			)
			.is_some(),
			"{}",
			true
		);

		let mut invalid_sig: Vec<u8> = Vec::new();
		invalid_sig.extend_from_slice(&[0u8, 64]);
		let signed_proposal = Proposal::Signed {
			kind: ProposalKind::EVM,
			data: tx_v_2.encode(),
			signature: invalid_sig.clone(),
		};

		// it does not return an error, however the proposal is not added to the list.
		// This is because the signature is invalid, and we are batch processing.
		// we could check for the RuntimeEvent that is emitted.
		assert_ok!(DKGProposalHandler::submit_signed_proposals(
			RuntimeOrigin::signed(sr25519::Public::from_raw([1; 32])),
			vec![signed_proposal]
		));

		assert!(
			DKGProposalHandler::unsigned_proposals(
				TypedChainId::Evm(0),
				DKGPayloadKey::EVMProposal(0.into())
			)
			.is_some(),
			"{}",
			true
		);
		assert!(
			DKGProposalHandler::signed_proposals(
				TypedChainId::Evm(0),
				DKGPayloadKey::EVMProposal(0.into())
			)
			.is_none(),
			"{}",
			true
		);
	});
}

pub fn make_header(chain: TypedChainId) -> ProposalHeader {
	match chain {
		TypedChainId::Evm(_) => ProposalHeader::new(
			[
				1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 1, 0,
				0, 0, 0, 1,
			]
			.into(),
			[0x26, 0x57, 0x88, 0x01].into(),
			1.into(),
		),
		TypedChainId::Substrate(_) => ProposalHeader::new(
			[
				1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 2, 0,
				0, 0, 0, 1,
			]
			.into(),
			[0x0, 0x0, 0x0, 0x0].into(),
			1.into(),
		),
		_ => {
			// Dummy Header
			ProposalHeader::new(
				[
					1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0,
					0, 0, 0, 0, 0, 1,
				]
				.into(),
				[0x0, 0x0, 0x0, 0x0].into(),
				1.into(),
			)
		},
	}
}

pub fn make_proposal<const N: usize>(prop: Proposal, chain: TypedChainId) -> Proposal {
	// Create the proposal Header
	let header = make_header(chain);
	let mut buf = vec![];
	header.encode_to(&mut buf);
	// N bytes parameter
	buf.extend_from_slice(&[0u8; N]);

	if let Proposal::Unsigned { kind, .. } = prop {
		return match kind {
			ProposalKind::TokenAdd => Proposal::Unsigned { kind, data: buf },
			ProposalKind::TokenRemove => Proposal::Unsigned { kind, data: buf },
			ProposalKind::WrappingFeeUpdate => Proposal::Unsigned { kind, data: buf },
			ProposalKind::ResourceIdUpdate => Proposal::Unsigned { kind, data: buf },
			ProposalKind::AnchorCreate => Proposal::Unsigned { kind, data: buf },
			ProposalKind::AnchorUpdate => Proposal::Unsigned { kind, data: buf },
			ProposalKind::RescueTokens => Proposal::Unsigned { kind, data: buf },
			ProposalKind::MaxDepositLimitUpdate => Proposal::Unsigned { kind, data: buf },
			ProposalKind::MinWithdrawalLimitUpdate => Proposal::Unsigned { kind, data: buf },
			ProposalKind::SetTreasuryHandler => Proposal::Unsigned { kind, data: buf },
			ProposalKind::SetVerifier => Proposal::Unsigned { kind, data: buf },
			ProposalKind::FeeRecipientUpdate => Proposal::Unsigned { kind, data: buf },
			_ => panic!("Invalid proposal type"),
		}
	}

	panic!("Invalid proposal type")
}

#[test]
fn force_submit_should_fail_with_invalid_proposal_type() {
	execute_test_with(|| {
		assert_err!(
			DKGProposalHandler::force_submit_unsigned_proposal(
				RuntimeOrigin::root(),
				Proposal::Unsigned { kind: ProposalKind::AnchorUpdate, data: vec![] }
			),
			crate::Error::<Test>::ProposalFormatInvalid
		);
	});
}

#[test]
fn force_submit_should_work_with_valid_proposals() {
	execute_test_with(|| {
		// EVM Tests
		assert_ok!(DKGProposalHandler::force_submit_unsigned_proposal(
			RuntimeOrigin::root(),
			make_proposal::<20>(
				Proposal::Unsigned { kind: ProposalKind::TokenAdd, data: vec![] },
				TypedChainId::Evm(0)
			)
		));
		assert!(
			DKGProposalHandler::unsigned_proposals(
				TypedChainId::Evm(1),
				DKGPayloadKey::TokenAddProposal(1.into())
			)
			.is_some(),
			"{}",
			true
		);
		assert_ok!(DKGProposalHandler::force_submit_unsigned_proposal(
			RuntimeOrigin::root(),
			make_proposal::<20>(
				Proposal::Unsigned { kind: ProposalKind::TokenRemove, data: vec![] },
				TypedChainId::Evm(0)
			)
		));
		assert!(
			DKGProposalHandler::unsigned_proposals(
				TypedChainId::Evm(1),
				DKGPayloadKey::TokenRemoveProposal(1.into())
			)
			.is_some(),
			"{}",
			true
		);
		assert_ok!(DKGProposalHandler::force_submit_unsigned_proposal(
			RuntimeOrigin::root(),
			make_proposal::<2>(
				Proposal::Unsigned { kind: ProposalKind::WrappingFeeUpdate, data: vec![] },
				TypedChainId::Evm(0)
			)
		));
		assert!(
			DKGProposalHandler::unsigned_proposals(
				TypedChainId::Evm(1),
				DKGPayloadKey::WrappingFeeUpdateProposal(1.into())
			)
			.is_some(),
			"{}",
			true
		);
		assert_ok!(DKGProposalHandler::force_submit_unsigned_proposal(
			RuntimeOrigin::root(),
			make_proposal::<72>(
				Proposal::Unsigned { kind: ProposalKind::RescueTokens, data: vec![] },
				TypedChainId::Evm(0)
			)
		));
		assert!(
			DKGProposalHandler::unsigned_proposals(
				TypedChainId::Evm(1),
				DKGPayloadKey::RescueTokensProposal(1.into())
			)
			.is_some(),
			"{}",
			true
		);
		assert_ok!(DKGProposalHandler::force_submit_unsigned_proposal(
			RuntimeOrigin::root(),
			make_proposal::<52>(
				Proposal::Unsigned { kind: ProposalKind::ResourceIdUpdate, data: vec![] },
				TypedChainId::Evm(0)
			)
		));
		assert!(
			DKGProposalHandler::unsigned_proposals(
				TypedChainId::Evm(1),
				DKGPayloadKey::ResourceIdUpdateProposal(1.into())
			)
			.is_some(),
			"{}",
			true
		);

		assert_ok!(DKGProposalHandler::force_submit_unsigned_proposal(
			RuntimeOrigin::root(),
			make_proposal::<32>(
				Proposal::Unsigned { kind: ProposalKind::MaxDepositLimitUpdate, data: vec![] },
				TypedChainId::Evm(0)
			)
		));
		assert!(
			DKGProposalHandler::unsigned_proposals(
				TypedChainId::Evm(1),
				DKGPayloadKey::MaxDepositLimitUpdateProposal(1.into())
			)
			.is_some(),
			"{}",
			true
		);

		assert_ok!(DKGProposalHandler::force_submit_unsigned_proposal(
			RuntimeOrigin::root(),
			make_proposal::<32>(
				Proposal::Unsigned { kind: ProposalKind::MinWithdrawalLimitUpdate, data: vec![] },
				TypedChainId::Evm(0)
			)
		));
		assert!(
			DKGProposalHandler::unsigned_proposals(
				TypedChainId::Evm(1),
				DKGPayloadKey::MinWithdrawalLimitUpdateProposal(1.into())
			)
			.is_some(),
			"{}",
			true
		);

		assert_ok!(DKGProposalHandler::force_submit_unsigned_proposal(
			RuntimeOrigin::root(),
			make_proposal::<20>(
				Proposal::Unsigned { kind: ProposalKind::SetTreasuryHandler, data: vec![] },
				TypedChainId::Evm(0)
			)
		));
		assert!(
			DKGProposalHandler::unsigned_proposals(
				TypedChainId::Evm(1),
				DKGPayloadKey::SetTreasuryHandlerProposal(1.into())
			)
			.is_some(),
			"{}",
			true
		);

		assert_ok!(DKGProposalHandler::force_submit_unsigned_proposal(
			RuntimeOrigin::root(),
			make_proposal::<20>(
				Proposal::Unsigned { kind: ProposalKind::SetVerifier, data: vec![] },
				TypedChainId::Evm(0)
			)
		));
		assert!(
			DKGProposalHandler::unsigned_proposals(
				TypedChainId::Evm(1),
				DKGPayloadKey::SetVerifierProposal(1.into())
			)
			.is_some(),
			"{}",
			true
		);

		assert_ok!(DKGProposalHandler::force_submit_unsigned_proposal(
			RuntimeOrigin::root(),
			make_proposal::<20>(
				Proposal::Unsigned { kind: ProposalKind::FeeRecipientUpdate, data: vec![] },
				TypedChainId::Evm(0)
			)
		));
		assert!(
			DKGProposalHandler::unsigned_proposals(
				TypedChainId::Evm(1),
				DKGPayloadKey::FeeRecipientUpdateProposal(1.into())
			)
			.is_some(),
			"{}",
			true
		);

		// Substrate Tests
		assert_ok!(DKGProposalHandler::force_submit_unsigned_proposal(
			RuntimeOrigin::root(),
			make_proposal::<20>(
				Proposal::Unsigned { kind: ProposalKind::TokenAdd, data: vec![] },
				TypedChainId::Substrate(0)
			)
		));
		assert!(
			DKGProposalHandler::unsigned_proposals(
				TypedChainId::Substrate(1),
				DKGPayloadKey::TokenAddProposal(1.into())
			)
			.is_some(),
			"{}",
			true
		);
		assert_ok!(DKGProposalHandler::force_submit_unsigned_proposal(
			RuntimeOrigin::root(),
			make_proposal::<20>(
				Proposal::Unsigned { kind: ProposalKind::TokenRemove, data: vec![] },
				TypedChainId::Substrate(0)
			)
		));
		assert!(
			DKGProposalHandler::unsigned_proposals(
				TypedChainId::Substrate(1),
				DKGPayloadKey::TokenRemoveProposal(1.into())
			)
			.is_some(),
			"{}",
			true
		);
		assert_ok!(DKGProposalHandler::force_submit_unsigned_proposal(
			RuntimeOrigin::root(),
			make_proposal::<1>(
				Proposal::Unsigned { kind: ProposalKind::WrappingFeeUpdate, data: vec![] },
				TypedChainId::Substrate(0)
			)
		));
		assert!(
			DKGProposalHandler::unsigned_proposals(
				TypedChainId::Substrate(1),
				DKGPayloadKey::WrappingFeeUpdateProposal(1.into())
			)
			.is_some(),
			"{}",
			true
		);
		assert_ok!(DKGProposalHandler::force_submit_unsigned_proposal(
			RuntimeOrigin::root(),
			make_proposal::<20>(
				Proposal::Unsigned { kind: ProposalKind::AnchorCreate, data: vec![] },
				TypedChainId::Substrate(0)
			)
		));
		assert!(
			DKGProposalHandler::unsigned_proposals(
				TypedChainId::Substrate(1),
				DKGPayloadKey::AnchorCreateProposal(1.into())
			)
			.is_some(),
			"{}",
			true
		);
		assert_ok!(DKGProposalHandler::force_submit_unsigned_proposal(
			RuntimeOrigin::root(),
			make_proposal::<20>(
				Proposal::Unsigned { kind: ProposalKind::AnchorUpdate, data: vec![] },
				TypedChainId::Substrate(0)
			)
		));
		assert!(
			DKGProposalHandler::unsigned_proposals(
				TypedChainId::Substrate(1),
				DKGPayloadKey::AnchorUpdateProposal(1.into())
			)
			.is_some(),
			"{}",
			true
		);
		assert_ok!(DKGProposalHandler::force_submit_unsigned_proposal(
			RuntimeOrigin::root(),
			make_proposal::<72>(
				Proposal::Unsigned { kind: ProposalKind::ResourceIdUpdate, data: vec![] },
				TypedChainId::Substrate(0)
			)
		));
		assert!(
			DKGProposalHandler::unsigned_proposals(
				TypedChainId::Substrate(1),
				DKGPayloadKey::ResourceIdUpdateProposal(1.into())
			)
			.is_some(),
			"{}",
			true
		);
	});
}

#[test]
fn expired_unsigned_proposals_are_removed() {
	execute_test_with(|| {
		// Submit one unsigned proposal
		assert_ok!(DKGProposalHandler::force_submit_unsigned_proposal(
			RuntimeOrigin::root(),
			make_proposal::<20>(
				Proposal::Unsigned { kind: ProposalKind::TokenAdd, data: vec![] },
				TypedChainId::Evm(0)
			)
		));

		// lets time travel to 5 blocks later and submit another proposal
		run_n_blocks(5);
		assert_ok!(DKGProposalHandler::force_submit_unsigned_proposal(
			RuntimeOrigin::root(),
			make_proposal::<1>(
				Proposal::Unsigned { kind: ProposalKind::WrappingFeeUpdate, data: vec![] },
				TypedChainId::Substrate(0)
			)
		));

		// sanity check
		run_n_blocks(10);
		assert_eq!(UnsignedProposalQueue::<Test>::iter().count(), 2);

		// lets time travel to a block after expiry period of first unsigned
		run_n_blocks(11);
		assert_eq!(UnsignedProposalQueue::<Test>::iter().count(), 1);

		// lets time travel to a block after expiry period of second unsigned
		run_n_blocks(16);
		assert_eq!(UnsignedProposalQueue::<Test>::iter().count(), 0);
	})
}
