@pushd "%~dp0.."
cd "%~dp0..\jimage-sys" && cargo publish %* || goto :err
@ping localhost -n 11 >NUL 2>NUl
cd "%~dp0..\jimage"     && cargo publish %* || goto :err
:err
@popd && exit /b %ERRORLEVEL%
