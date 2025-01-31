# DA dispatcher

This crate contains an implementation of the DataAvailability dispatcher component, which sends a blobs of data to the
corresponding DA layer.

## Overview

The implementation of the DA clients is abstracted away from the dispatcher. The dispatcher is responsible for storing
the DA blobs info in the Postgres database and use it to get the inclusion proofs for the blobs. The retries logic is
also part of the DA dispatcher.

This component assumes that batches are being sent to the L1 sequentially and that there is no need to fetch the
inclusion data for their DA in parallel. Same with dispatching DA blobs, there is no need to do that in parallel unless
we are facing performance issues when the sequencer is trying to catch up after some outage.

This is a singleton component, only one instance of the DA dispatcher should be running at a time. In case multiple
instances are started, they will be dispatching the same pubdata blobs to the DA layer. It is not going to cause any
critical issues, but it is wasteful.
