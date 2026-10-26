use baierr::protocol;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

fn tmp_path(name: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("/tmp/opencode/bench_{}_{}.b", name, nanos)
}

fn bench_single_append(c: &mut Criterion) {
    c.bench_function("append_single_1kb", |b| {
        b.iter(|| {
            let p = tmp_path("single");
            protocol::create(&p).unwrap();
            let data = vec![b'x'; 1024];
            protocol::append(&p, &data, "foo").unwrap();
            let _ = fs::remove_file(&p);
            black_box(());
        })
    });
}

fn bench_multi_append(c: &mut Criterion) {
    c.bench_function("append_100x_64b", |b| {
        b.iter(|| {
            let p = tmp_path("multi");
            protocol::create(&p).unwrap();
            for i in 0..100 {
                let data = vec![b'y'; 64];
                let name = format!("f{}", i);
                protocol::append(&p, &data, &name).unwrap();
            }
            let _ = fs::remove_file(&p);
            black_box(());
        })
    });
}

fn bench_read(c: &mut Criterion) {
    c.bench_function("read_after_100_appends", |b| {
        let p = tmp_path("read");
        protocol::create(&p).unwrap();
        for i in 0..100 {
            let data = vec![b'z'; 64];
            let name = format!("f{}", i);
            protocol::append(&p, &data, &name).unwrap();
        }
        b.iter(|| {
            let d = protocol::read(&p, "f50").unwrap();
            black_box(d);
        });
        let _ = fs::remove_file(&p);
    });
}

fn bench_get_rdd(c: &mut Criterion) {
    c.bench_function("get_rdd_100", |b| {
        let p = tmp_path("rdd");
        protocol::create(&p).unwrap();
        for i in 0..100 {
            let data = vec![b'w'; 64];
            let name = format!("f{}", i);
            protocol::append(&p, &data, &name).unwrap();
        }
        b.iter(|| {
            let r = protocol::get_rdd(&p).unwrap();
            black_box(r);
        });
        let _ = fs::remove_file(&p);
    });
}

criterion_group!(
    benches,
    bench_single_append,
    bench_multi_append,
    bench_read,
    bench_get_rdd
);
criterion_main!(benches);