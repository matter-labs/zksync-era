# Data Availability Clients

This crate contains the implementations of the Data Availability clients.

Currently, the following DataAvailability clients are implemented:

- `NoDA client` that does not send or store any pubdata, it is needed to run the zkSync network in the "no-DA" mode
  utilizing the DA framework.
- `Object Store client` that stores the pubdata in the Object Store(GCS).
- `Avail` that sends the pubdata to the Avail DA layer.
- `Celestia` that sends the pubdata to the Celestia DA layer.
- `Eigen` that sends the pubdata to the Eigen DA layer.
