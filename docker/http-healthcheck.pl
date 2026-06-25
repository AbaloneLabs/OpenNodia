#!/usr/bin/perl

use strict;
use warnings;
use IO::Socket::INET;

my ($host, $port, $path) = @ARGV;
$host //= "127.0.0.1";
$path //= "/health";

die "usage: http-healthcheck.pl HOST PORT [PATH]\n"
    unless defined $port && $port =~ /^\d+$/;

my $socket = IO::Socket::INET->new(
    PeerAddr => $host,
    PeerPort => $port,
    Proto    => "tcp",
    Timeout  => 3,
) or die "connection failed: $!\n";

print {$socket} "GET $path HTTP/1.0\r\n"
    . "Host: $host:$port\r\n"
    . "Connection: close\r\n\r\n";

my $status = <$socket> // "";
close $socket;

exit($status =~ m{^HTTP/\d+(?:\.\d+)? 200(?:\s|$)} ? 0 : 1);
