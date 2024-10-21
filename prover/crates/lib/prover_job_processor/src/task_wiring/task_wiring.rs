//
// // SaverTask implementation
// pub struct SaverTask<S>
// where
//     S: JobSaver,
// {
//     saver: Arc<S>,
//     result_rx: mpsc::Receiver<Result<S::Output>>,
// }
//
// impl<S> SaverTask<S>
// where
//     S: JobSaver,
// {
//     pub fn new(saver: Arc<S>, result_rx: mpsc::Receiver<Result<S::Output>>) -> Self {
//         Self { saver, result_rx }
//     }
// }
//
// #[async_trait]
// impl<S> Task for SaverTask<S>
// where
//     S: JobSaver,
// {
//     async fn run(self: Arc<Self>) -> Result<()> {
//         while let Some(result) = self.result_rx.recv().await {
//             if let Err(e) = self.saver.save_result(result).await {
//                 eprintln!("Error saving result: {:?}", e);
//             }
//         }
//         Ok(())
//     }
// }
