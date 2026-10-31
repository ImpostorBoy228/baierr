use baierr::protocol;
use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

const KEY: [u8; 32] = [0x2a; 32];

fn tmp_path(name: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("/tmp/opencode/bench_{name}_{nanos}.b")
}

fn del(p: &str) {
    let _ = fs::remove_file(p);
}

/// создать архив с `entries` записями по `data_size` байт
fn make_archive(p: &str, entries: usize, data_size: usize) {
    protocol::create(p).unwrap();
    let data = vec![b'a'; data_size];
    for i in 0..entries {
        protocol::append(p, &KEY, &data, &format!("f{i:06}")).unwrap();
    }
}

// ============================================================
// append: полный размерный ряд от overhead-floor до 1MB
// ============================================================

fn bench_append_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("append");
    for &size in [0usize, 8, 1024, 65_536, 1_048_576].iter() {
        group.throughput(Throughput::Bytes(size.max(8) as u64));
        group.bench_function(format!("append_{size}b"), |b| {
            b.iter(|| {
                let p = tmp_path("as");
                protocol::create(&p).unwrap();
                let data = vec![b'x'; size];
                protocol::append(&p, &KEY, &data, "foo").unwrap();
                let _ = fs::remove_file(&p);
                black_box(());
            })
        });
    }
    group.finish();
}

fn bench_append_long_name(c: &mut Criterion) {
    c.bench_function("append_long_name_10kb", |b| {
        let name = "f".repeat(10_000);
        b.iter(|| {
            let p = tmp_path("aln");
            protocol::create(&p).unwrap();
            protocol::append(&p, &KEY, b"x", &name).unwrap();
            let _ = fs::remove_file(&p);
            black_box(());
        })
    });
}

/// append в архив, где уже 1000 записей: каждая запись тянет за собой
/// переписывание rdd-региона + полный скан файла от конца — растёт как O(архив)
fn bench_append_into_growing_archive(c: &mut Criterion) {
    c.bench_function("append_into_1000_entries", |b| {
        b.iter_batched(
            || {
                let p = tmp_path("into1k");
                make_archive(&p, 1000, 64);
                p
            },
            |p| {
                let data = vec![b'y'; 256];
                protocol::append(&p, &KEY, &data, "new-entry").unwrap();
                del(&p);
                black_box(());
            },
            BatchSize::PerIteration,
        )
    });
}

// ============================================================
// read: Locality первый/последний в большом rdd + размеры
// ============================================================

fn bench_read_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("read");
    for &size in [8usize, 1024, 65_536, 1_048_576].iter() {
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_function(format!("read_{size}b"), |b| {
            let p = tmp_path("rs");
            protocol::create(&p).unwrap();
            let data = vec![b'z'; size];
            protocol::append(&p, &KEY, &data, "foo").unwrap();
            b.iter(|| {
                let d = protocol::read(&p, &KEY, "foo").unwrap();
                black_box(d);
            });
            del(&p);
        });
    }
    group.finish();
}

fn bench_read_locality_in_1000_entries(c: &mut Criterion) {
    let p = tmp_path("loc");
    make_archive(&p, 1000, 64);
    c.bench_function("read_first_of_1000", |b| {
        b.iter(|| {
            let d = protocol::read(&p, &KEY, "f000000").unwrap();
            black_box(d);
        })
    });
    c.bench_function("read_last_of_1000", |b| {
        b.iter(|| {
            let d = protocol::read(&p, &KEY, "f000999").unwrap();
            black_box(d);
        })
    });
    c.bench_function("read_all_1000", |b| {
        b.iter(|| {
            for i in 0..1000 {
                black_box(protocol::read(&p, &KEY, &format!("f{i:06}")).unwrap());
            }
        })
    });
    del(&p);
}

// ============================================================
// get_rdd: плоский список металовности
// ============================================================

fn bench_get_rdd(c: &mut Criterion) {
    let mut group = c.benchmark_group("get_rdd");
    for &entries in [0usize, 100, 1000].iter() {
        let p = tmp_path("rddb");
        make_archive(&p, entries, 64);
        group.bench_function(format!("get_rdd_{entries}"), |b| {
            b.iter(|| {
                let r = protocol::get_rdd(&p).unwrap();
                black_box(r);
            })
        });
        del(&p);
    }
    group.finish();
}

// ============================================================
// интерлив (видимая нагрузка CLI): по 32 байтовых запись через 1kb
// ============================================================

// === bench_read_during_growth ===
// read_64b во время роста архива: append + read по кругу
fn bench_interleaved_read_write(c: &mut Criterion) {
    c.bench_function("interleaved_append_read", |b| {
        b.iter(|| {
            let p = tmp_path("iaw");
            protocol::create(&p).unwrap();
            for i in 0..10 {
                let data = vec![b'w'; 1024];
                let name = format!("f{i}");
                protocol::append(&p, &KEY, &data, &name).unwrap();
                black_box(protocol::read(&p, &KEY, &name).unwrap());
            }
            del(&p);
            black_box(());
        })
    });
}

criterion_group!(
    benches,
    bench_append_sizes,
    bench_append_long_name,
    bench_append_into_growing_archive,
    bench_read_sizes,
    bench_read_locality_in_1000_entries,
    bench_get_rdd,
    bench_interleaved_read_write
);
criterion_main!(benches);
