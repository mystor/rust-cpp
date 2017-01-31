set -e
WD=$PWD
cd $WD/cpp && cargo build
cd $WD/cpp_common && cargo build
cd $WD/cpp_build && cargo build
cd $WD/cpp_macros && cargo build
cd $WD/test && cargo test
