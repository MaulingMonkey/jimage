@pushd "%~dp0.."
cargo build --all || goto :err
cargo test  --all || goto :err
cargo build --all --target=i686-pc-windows-msvc || goto :err
cargo test  --all --target=i686-pc-windows-msvc || goto :err
cd "%~dp0..\jimage"     && cargo +nightly doc --no-deps --features nightly || goto :err
cd "%~dp0..\jimage-sys" && cargo +nightly doc --no-deps --features nightly || goto :err
@where wsl >NUL 2>NUL && wsl bash --login -c scripts/test.sh
:err
@popd && exit /b %ERRORLEVEL%
