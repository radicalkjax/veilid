library;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:veilid/veilid.dart';
import 'package:veilid_integration_test/veilid_integration_test.dart';
import 'package:veilid_test/veilid_test.dart';
// import 'package:test_api/src/backend/invoker.dart';

import 'test_crypto.dart';
import 'test_dht.dart';
import 'test_dht_transactions.dart';
import 'test_dht_transactions_full.dart';
import 'test_routing_context.dart';
import 'test_table_db.dart';
import 'test_veilid_config.dart';

void main() => runWithIntegrationLog(() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  runApp(const IntegrationLogApp());

  final fixture = DefaultVeilidFixture(
      programName: 'veilid_flutter integration test',
      logFixture: LogFixture.instance);
  final updateProcessorFixture =
      UpdateProcessorFixture(veilidFixture: fixture);

  // setUp(() {
  //   final liveTest = Invoker.current?.liveTest;
  //   print('setUp ${liveTest?.test.name}');
  // });
  // tearDown(() {
  //   final liveTest = Invoker.current?.liveTest;
  //   print('tearDown ${liveTest?.test.name}');
  // });

  group('Uninitialized', () {
    test('veilid config defaults', testVeilidConfigDefaults, tags: ['config']);
  });

  group('Initialized', () {
    setUpAll(fixture.setUp);
    tearDownAll(fixture.tearDown);
    setUpAll(updateProcessorFixture.setUp);
    tearDownAll(updateProcessorFixture.tearDown);
    // Fail the run if any subsystem (online/relay/route/connection/readiness) flapped
    tearDownAll(LogFixture.instance.assertNoFlapping);

    group('Crypto', () {
      test('list cryptosystems', testListCryptoSystems, tags: ['crypto']);
      test('get cryptosystem', testGetCryptoSystems, tags: ['crypto']);
      test('get cryptosystem invalid', testGetCryptoSystemInvalid,
          tags: ['crypto']);
      test('hash and verify password', testHashAndVerifyPassword,
          tags: ['crypto']);
      test('sign and verify signature', testSignAndVerifySignature);
      test('sign and verify signatures', testSignAndVerifySignatures,
          tags: ['crypto']);
      test('generate shared secret', testGenerateSharedSecret,
          tags: ['crypto']);
      test('hpke seal and open', testHpkeSealOpen, tags: ['crypto']);
      test('kem bridge from signing keys', testKemBridgeFromSigningKeys,
          tags: ['crypto']);
    });

    group('Table DB', () {
      test('delete table db nonexistent', testDeleteTableDbNonExistent,
          tags: ['table_db']);
      test('open delete table db', testOpenDeleteTableDb, tags: ['table_db']);
      test('open twice table db', testOpenTwiceTableDb, tags: ['table_db']);
      test('open twice table db store load', testOpenTwiceTableDbStoreLoad,
          tags: ['table_db']);
      test('open twice table db store delete load',
          testOpenTwiceTableDbStoreDeleteLoad,
          tags: ['table_db']);
      test('resize table db', testResizeTableDb, tags: ['table_db']);
    });

    group('Attached', () {
      setUpAll(fixture.attach);
      tearDownAll(fixture.detach);

      group('Routing Contexts', () {
        test('routing contexts', testRoutingContexts);
        test('app message loopback',
            () => testAppMessageLoopback(fixture.updateStream),
            tags: ['routing_context', 'app_message']);
        test('app call loopback',
            () => testAppCallLoopback(fixture.updateStream),
            tags: ['routing_context', 'app_call']);
        test('app message loopback big packets',
            () => testAppMessageLoopbackBigPackets(fixture.updateStream),
            tags: ['routing_context', 'app_message_big']);
        test('app call loopback big packets',
            () => testAppCallLoopbackBigPackets(fixture.updateStream),
            tags: ['routing_context', 'app_call_big']);
      });

      for (final cryptoKind in Veilid.instance.validCryptoKinds()) {
        group('DHT $cryptoKind', () {
          final testDHT = TestDHT(cryptoKind);
          setUpAll(testDHT.setUpAll);
          tearDownAll(testDHT.tearDownAll);

          test('get dht value unopened', testDHT.testGetDHTValueUnopened,
              tags: ['dht']);
          test('open dht record nonexistent no writer',
              testDHT.testOpenDHTRecordNonexistentNoWriter,
              tags: ['dht']);
          test('close dht record nonexistent',
              testDHT.testCloseDHTRecordNonexistent,
              tags: ['dht']);
          test('delete dht record nonexistent',
              testDHT.testDeleteDHTRecordNonexistent,
              tags: ['dht']);
          test('create delete dht record simple',
              testDHT.testCreateDeleteDHTRecordSimple,
              tags: ['dht']);
          test('create delete dht record no close',
              testDHT.testCreateDeleteDHTRecordNoClose,
              tags: ['dht']);
          test('create delete dht record with deterministic key',
              testDHT.testCreateDHTRecordWithDeterministicKey,
              tags: ['dht']);
          test('get dht value nonexistent', testDHT.testGetDHTValueNonexistent);
          test('set get dht value', testDHT.testSetGetDHTValue, tags: ['dht']);
          test('set get dht value with owner',
              testDHT.testSetGetDHTValueWithOwner,
              tags: ['dht']);
          test('open writer dht value', testDHT.testOpenWriterDHTValue,
              tags: ['dht']);
          test('inspect dht record', testDHT.testInspectDHTRecord,
              tags: ['dht']);
          test('flush dht record', testDHT.testFlushDHTRecord, tags: ['dht']);
        });

        group('DHT Transactions $cryptoKind', () {
          final testDHTTransactions =
              TestDHTTransactions(cryptoKind, updateProcessorFixture);
          setUpAll(testDHTTransactions.setUpAll);
          tearDownAll(testDHTTransactions.tearDownAll);

          setUp(testDHTTransactions.setUp);
          tearDown(testDHTTransactions.tearDown);

          test('should create empty transaction and drop it explicitly',
              testDHTTransactions.testEmptyTxAndDrop,
              tags: ['dht_transactions']);
          test('should create empty transaction and rollback',
              testDHTTransactions.testEmptyTxAndRollback,
              tags: ['dht_transactions']);
          test('should create empty transaction and commit',
              testDHTTransactions.testEmptyTxAndCommit,
              tags: ['dht_transactions']);
          test('should create transaction, add sets, and rollback',
              testDHTTransactions.testTxAddSetsAndRollback,
              tags: ['dht_transactions']);
          test('should create transaction, add sets, and commit',
              testDHTTransactions.testTxAddSetsAndCommit,
              tags: ['dht_transactions']);
          test('should extend transaction with new record and commit',
              testDHTTransactions.testTxExtendAndCommit,
              tags: ['dht_transactions']);
          test('should extend transaction with new record and rollback',
              testDHTTransactions.testTxExtendAndRollback,
              tags: ['dht_transactions']);
          test('should create transaction, add sets, gets, and commit',
              testDHTTransactions.testTxAddSetsGetsAndCommit,
              tags: ['dht_transactions']);
          test(
              'should create empty transaction, fail non-transactional sets and'
              ' then rollback',
              testDHTTransactions.testTxFailNonTxSetsAndRollback,
              tags: ['dht_transactions']);
          test(
              'should create transaction, inspect, add sets to subkey 1, '
              ' inspect and commit. Then a new transaction inspect, and gets, '
              'and commit',
              testDHTTransactions.testTxInspectAddInspectCommit,
              tags: ['dht_transactions']);
        });

        group('Veilid DHT Transactions Full $cryptoKind', () {
          final testDHTTransactionsFull = TestDHTTransactionsFull(cryptoKind);
          setUpAll(testDHTTransactionsFull.setUpAll);
          tearDownAll(testDHTTransactionsFull.tearDownAll);

          setUp(testDHTTransactionsFull.setUp);
          tearDown(testDHTTransactionsFull.tearDown);

          test('should create empty transaction, inspect, add gets and commit',
              testDHTTransactionsFull.testEmptyTxInspectGetsAndCommit,
              timeout: const Timeout(Duration(seconds: 120)), tags: ['stress']);
          test(
              'should create transaction, fill all records, commit, and then'
              ' get all records and rollback',
              testDHTTransactionsFull.testTxFillCommitTxGetRollback,
              timeout: const Timeout(Duration(seconds: 300)),
              tags: ['stress']);
        });
      }
    });
  });
});
