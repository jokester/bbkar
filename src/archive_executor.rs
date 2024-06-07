use crate::archive::Archive;
use crate::snapshot::Snapshot;
use opendal::{Scheme, layers, BlockingOperator};
use opendal::{Operator, Result as R};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone)]
struct ArchiveManagerConf {
    service: String,
    service_args: HashMap<String, String>,
    path_prefix: String
}

struct ArchiveManager {
    op: BlockingOperator,
    conf: ArchiveManagerConf
}

impl ArchiveManager {
    fn new(conf: ArchiveManagerConf) -> R<ArchiveManager> {
        let scheme = conf.service.parse::<Scheme>()?;
        let op = Operator::via_map(scheme, conf.service_args.clone())?.layer(layers::LoggingLayer::default()).blocking();
        Ok(ArchiveManager {op, conf})
    }

    fn list_archive(&self) -> R<Vec<Archive>> {
        todo!()
    }

    fn load_archive(a: Archive, dest: &PathBuf /* TODO: options */) {
        todo!()
    }

    fn save_snapshot(s: Snapshot, parent: Option<Snapshot>) -> R<()> {
        todo!()
    }

    fn ranged_read(&self) {
        let f = self.op.read_with("path").range(0..1024).call()?;
    }
}
