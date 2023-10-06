use super::*;

use sc_keystore::LocalKeystore;
use sp_consensus_hotstuff::{AuthorityId, AuthorityList, HOTSTUFF_KEY_TYPE};
use sp_keystore::KeystorePtr;
use sp_runtime::testing::{Header as TestHeader, TestXt};

type TestExtrinsic = TestXt<(), ()>;
type TestBlock = sp_runtime::testing::Block<TestExtrinsic>;

fn generate_sr25519_authorities(num: usize, keystore: &KeystorePtr) -> Vec<AuthorityId> {
	let mut authorities = Vec::new();
	for i in 0..num {
		let authority_id = keystore
			.sr25519_generate_new(HOTSTUFF_KEY_TYPE, Some(format!("//User{}", i).as_str()))
			.expect("Creates authority pair")
			.into();
		authorities.push(authority_id);
	}

	authorities
}

fn generate_proposal_with_block(
	keystore: KeystorePtr,
	signer: &AuthorityId,
	block: &TestBlock,
	view: ViewNumber,
) -> Proposal<TestBlock> {
	let mut proposal = Proposal::<TestBlock>::new(
		QC::<TestBlock>::default(),
		None,
		block.hash(),
		view,
		signer.clone(),
		None,
	);

	proposal.signature = Some(
		keystore
			.sr25519_sign(HOTSTUFF_KEY_TYPE, signer.as_ref(), proposal.digest().as_bytes())
			.unwrap()
			.unwrap()
			.into(),
	);

	proposal
}

fn generate_vote_with_proposal(
	keystore: KeystorePtr,
	signer: &AuthorityId,
	proposal: &Proposal<TestBlock>,
	view: ViewNumber,
) -> Vote<TestBlock> {
	let mut vote = Vote::<TestBlock> {
		proposal_hash: proposal.digest(),
		view,
		voter: signer.clone(),
		signature: None,
	};

	vote.signature = Some(
		keystore
			.sr25519_sign(HOTSTUFF_KEY_TYPE, signer.as_ref(), vote.digest().as_bytes())
			.unwrap()
			.unwrap()
			.into(),
	);

	vote
}

struct TestEnv {
	keystore: KeystorePtr,
	pks: Vec<AuthorityId>,
	weighted_authorities: Vec<(AuthorityId, u64)>,
	test_block: TestBlock,
	view: ViewNumber,
}

fn create_test_env() -> TestEnv {
	let keystore_path = tempfile::tempdir().expect("Creates keystore path");
	let keystore: KeystorePtr = LocalKeystore::open(keystore_path.path(), None)
		.expect("Creates keystore")
		.into();

	let pks = generate_sr25519_authorities(4, &keystore);
	let authorities = &pks[0..3];

	let weighted_authorities =
		authorities.iter().map(|id| (id.clone(), 1)).collect::<AuthorityList>();

	let test_block = TestBlock { header: TestHeader::new_from_number(10), extrinsics: Vec::new() };

	TestEnv { keystore, pks, weighted_authorities, test_block, view: 10 }
}

#[test]
fn test_proposal_verify() {
	let TestEnv { keystore, pks, weighted_authorities, test_block, view } = create_test_env();

	let authorities =
		weighted_authorities.iter().map(|a| a.0.clone()).collect::<Vec<AuthorityId>>();

	struct TestCase {
		describe: String,
		proposal: Proposal<TestBlock>,
		result: Result<(), HotstuffError>,
	}

	let cases = [
		TestCase {
			describe: "Null signature".to_string(),
			proposal: Proposal::<TestBlock>::new(
				QC::<TestBlock>::default(),
				None,
				test_block.hash(),
				view,
				authorities[0].clone(),
				None,
			),
			result: Err(NullSignature),
		},
		TestCase {
			describe: "Invalid signature".to_string(),
			proposal: Proposal::<TestBlock>::new(
				QC::<TestBlock>::default(),
				None,
				test_block.hash(),
				view,
				authorities[1].clone(),
				Some(
					keystore
						.sr25519_sign(HOTSTUFF_KEY_TYPE, authorities[1].as_ref(), b"bad")
						.unwrap()
						.unwrap()
						.into(),
				),
			),
			result: Err(InvalidSignature(authorities[1].clone())),
		},
		TestCase {
			describe: "Proposer not a authority".to_string(),
			proposal: Proposal::<TestBlock>::new(
				QC::<TestBlock>::default(),
				None,
				test_block.hash(),
				view,
				pks[3].clone(),
				None,
			),
			result: Err(UnknownAuthority(pks[3].clone())),
		},
		TestCase {
			describe: "Normal Proposal".to_string(),
			proposal: generate_proposal_with_block(
				keystore.clone(),
				&authorities[1],
				&test_block,
				view,
			),
			result: Ok(()),
		},
	];

	for case in cases.iter() {
		assert_eq!(
			case.proposal.verify(&weighted_authorities),
			case.result,
			"proposal verify failed. {} ",
			case.describe
		)
	}
}

#[test]
fn test_vote_verify() {
	let TestEnv { keystore, pks, weighted_authorities, test_block, view } = create_test_env();

	let authorities =
		weighted_authorities.iter().map(|a| a.0.clone()).collect::<Vec<AuthorityId>>();

	let proposal =
		generate_proposal_with_block(keystore.clone(), &authorities[0], &test_block, view);

	struct TestCase {
		describe: String,
		vote: Vote<TestBlock>,
		result: Result<(), HotstuffError>,
	}

	let cases = [
		TestCase {
			describe: "Normal vote".to_string(),
			vote: generate_vote_with_proposal(keystore.clone(), &authorities[0], &proposal, view),
			result: Ok(()),
		},
		TestCase {
			describe: "Null signature".to_string(),
			vote: || -> Vote<TestBlock> {
				let mut vote =
					generate_vote_with_proposal(keystore.clone(), &authorities[0], &proposal, view);
				// discard signature
				vote.signature = None;

				vote
			}(),
			result: Err(NullSignature),
		},
		TestCase {
			describe: "Invalid signature".to_string(),
			vote: || -> Vote<TestBlock> {
				let mut vote =
					generate_vote_with_proposal(keystore.clone(), &authorities[0], &proposal, view);
				// discard signature
				vote.signature = Some(
					keystore
						.sr25519_sign(HOTSTUFF_KEY_TYPE, authorities[1].as_ref(), b"bad data")
						.unwrap()
						.unwrap()
						.into(),
				);

				vote
			}(),
			result: Err(NullSignature),
		},
		TestCase {
			describe: "Voter is not authority".to_string(),
			vote: || -> Vote<TestBlock> {
				let mut vote =
					generate_vote_with_proposal(keystore.clone(), &authorities[0], &proposal, view);
				// change a invalid voter
				vote.voter = pks[3].clone();

				vote
			}(),
			result: Err(UnknownAuthority(pks[3].clone())),
		},
	];

	for case in cases {
		assert_eq!(
			case.vote.verify(&weighted_authorities),
			case.result,
			"vote verify failed. {} ",
			case.describe
		)
	}
}

#[test]
fn test_qc_verify() {
	let TestEnv { keystore, weighted_authorities, test_block, view,.. } = create_test_env();

	let authorities =
		weighted_authorities.iter().map(|a| a.0.clone()).collect::<Vec<AuthorityId>>();

	let proposal =
		generate_proposal_with_block(keystore.clone(), &authorities[0], &test_block, view);

	struct TestCase {
		describe: String,
		qc: QC<TestBlock>,
		result: Result<(), HotstuffError>,
	}

	let cases = [TestCase {
		describe: "with quorum".to_string(),
		qc: QC::<TestBlock> {
			proposal_hash: proposal.digest(),
			view,
			votes: || -> Vec<(AuthorityId, AuthoritySignature)> {
				let mut votes = Vec::new();
				for authority_id in authorities.iter() {
					let vote = generate_vote_with_proposal(
						keystore.clone(),
						authority_id,
						&proposal,
						view,
					);
					votes.push((authority_id.to_owned(), vote.signature.unwrap()))
				}
				votes
			}(),
		},
		result: Ok(()),
	}];

	for case in cases {
		assert_eq!(
			case.qc.verify(&weighted_authorities),
			case.result,
			"qc verify failed. {} ",
			case.describe
		)
	}
}

#[test]
fn qc_from_votes_should_work() {
	let keystore_path = tempfile::tempdir().expect("Creates keystore path");
	let keystore: KeystorePtr = LocalKeystore::open(keystore_path.path(), None)
		.expect("Creates keystore")
		.into();

	let authorities = generate_sr25519_authorities(3, &keystore);

	let test_block = TestBlock { header: TestHeader::new_from_number(3), extrinsics: Vec::new() };

	let view_number = 1;

	let proposal = Proposal::<TestBlock> {
		qc: Default::default(),
		tc: None,
		payload: test_block.hash(),
		view: view_number,
		author: authorities[0].clone(),
		signature: None,
	};

	let proposal_digest = proposal.digest();

	let vote = Vote::<TestBlock> {
		proposal_hash: proposal_digest,
		view: view_number,
		voter: authorities[0].clone(),
		signature: None,
	};

	let qc =
		QC::<TestBlock> { proposal_hash: proposal_digest, view: view_number, votes: Vec::new() };

	assert_eq!(vote.digest(), qc.digest());
}
