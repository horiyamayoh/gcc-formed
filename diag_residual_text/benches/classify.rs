use criterion::{Criterion, black_box, criterion_group, criterion_main};
use diag_residual_text::classify;

const MIXED_STDERR: &str = "\
In file included from src/wrapper_a.h:1,\n\
                 from src/main.c:1:\n\
src/config_a.h:1:23: error: first missing symbol\n\
src/main.c:3:25: note: in expansion of macro 'FETCH_A'\n\
In file included from src/wrapper_b.h:2,\n\
                 from src/other.c:1:\n\
src/config_b.h:2:11: error: second missing symbol\n\
src/other.c:8:9: note: in expansion of macro 'FETCH_B'\n\
main.cpp:5:7: error: no matching function for call to 'takes(int)'\n\
main.cpp:2:6: note: candidate 1: 'void takes(int, int)'\n\
/usr/bin/ld: main.o: in function `main':\n\
main.c:(.text+0x15): undefined reference to `foo'\n\
collect2: error: ld returned 1 exit status\n";

fn bench_classify(c: &mut Criterion) {
    c.bench_function("classify_mixed_stderr", |b| {
        b.iter(|| classify(black_box(MIXED_STDERR), black_box(true)));
    });
}

criterion_group!(benches, bench_classify);
criterion_main!(benches);
