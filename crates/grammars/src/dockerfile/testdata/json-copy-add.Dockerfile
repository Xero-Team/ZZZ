FROM scratch
COPY ["file with spaces.txt", "/dest/file with spaces.txt"]
ADD ["https://example.com/archive.tar.gz", "/tmp/archive.tar.gz"]