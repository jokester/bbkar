fn () {
}

struct BtrfsAccessor {

}

struct StorageClient {
}

enum BbkarCommand {
    Info(),
    Backup(
        local_backups: List<Backu>,
        storage_client: StorageClient;
        ),
    Prune()
}

fn main() {
    let cmd = BbkarCommand;

    match cmd {
        In
    }
}

fn cmd_info(cmd: Info) {
}

fn cmd_backup(cmd: Backup) {
    let volumes_to_backup = list!( );
}

fn backup_single_volume_full(src, dest, executor) {

}

fn backup_single_volume_full(src_parent, src_child, dest, executor) {

}

fn load_state () {
    let local_state = list_local_volume();
    let remote_state = list_remote_volume();

    match f {
        when 


}