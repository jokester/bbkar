struct X {
    f: String

}

struct ArchiveStorage {

}

struct BbkarConfigFile {

}

struct LocalSnapshotRoot {
    path: str // typically a _btrbk_snap
    subvolume_prefix: str

    volumes: List<LocalSnapshot>
}

struct LocalSnapshot {
    basename: str
    timestamp: str;
}

struct RemoteVolumeRoot {
}
