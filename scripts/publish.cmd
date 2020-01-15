@pushd "%~dp0.."
cd "%~dp0..\jimage"     && cargo publish %* || goto :err
cd "%~dp0..\jimage-sys" && cargo publish %* || goto :err
:err
@popd && exit /b %ERRORLEVEL%
