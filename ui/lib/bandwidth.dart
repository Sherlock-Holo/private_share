import 'dart:convert';
import 'dart:math';

import 'package:flutter/material.dart';
import 'package:ui/bandwidth_response.dart';
import 'package:web_socket_channel/web_socket_channel.dart';

class Bandwidth extends StatefulWidget {
  const Bandwidth({super.key});

  @override
  State<StatefulWidget> createState() => _BandwidthState();
}

class _BandwidthState extends State<Bandwidth> {
  late final WebSocketChannel _webSocketChannel;
  int inbound = 0;
  int outbound = 0;

  static const double _minWidth = 90;

  @override
  void initState() {
    super.initState();

    final Uri u;
    final query = {"interval": "1000"};
    if (Uri.base.scheme == "http") {
      u = Uri.http(Uri.base.authority, "/api/get_bandwidth", query);
    } else {
      u = Uri.https(Uri.base.authority, "/api/get_bandwidth", query);
    }
    final url = u.toString().replaceFirst("http", "ws");

    _webSocketChannel = WebSocketChannel.connect(Uri.parse(url));
  }

  @override
  void dispose() {
    _webSocketChannel.sink.close();

    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return _buildBandwidthWidget();
  }

  Widget _buildBandwidthWidget() {
    return StreamBuilder(
      stream: _webSocketChannel.stream,
      builder: (context, snapshot) {
        if (snapshot.hasData) {
          final json =
              jsonDecode(snapshot.data!.toString()) as Map<String, dynamic>;
          final resp = GetBandWidthResponse.fromJson(json);
          final inboundSpeed = resp.inbound - inbound;
          final outboundSpeed = resp.outbound - outbound;
          inbound = resp.inbound;
          outbound = resp.outbound;

          return Wrap(
            spacing: 8,
            children: [
              Chip(
                avatar: const CircleAvatar(
                  child: Icon(Icons.download_rounded),
                ),
                label: Container(
                    alignment: Alignment.centerRight,
                    constraints: const BoxConstraints(
                        minWidth: _minWidth, maxWidth: _minWidth),
                    child: Text(_formatBytes(inboundSpeed, 2))),
              ),
              Chip(
                avatar: const CircleAvatar(
                  child: Icon(Icons.upload_rounded),
                ),
                label: Container(
                    alignment: Alignment.centerRight,
                    constraints: const BoxConstraints(
                        minWidth: _minWidth, maxWidth: _minWidth),
                    child: Text(_formatBytes(outboundSpeed, 2))),
              ),
            ],
          );
        }

        if (snapshot.hasError) {
          final err = snapshot.error!;

          return Wrap(
            spacing: 8,
            children: [
              Chip(
                avatar: const CircleAvatar(
                  child: Icon(Icons.download_rounded),
                ),
                label: Container(
                    alignment: Alignment.centerRight,
                    constraints: const BoxConstraints(
                        minWidth: _minWidth, maxWidth: _minWidth),
                    child: Text(
                      "$err",
                      style: const TextStyle(color: Colors.red),
                    )),
              ),
              Chip(
                  avatar: const CircleAvatar(
                    child: Icon(Icons.upload_rounded),
                  ),
                  label: Container(
                      alignment: Alignment.centerRight,
                      constraints: const BoxConstraints(
                          minWidth: _minWidth, maxWidth: _minWidth),
                      child: Text(
                        "$err",
                        style: const TextStyle(color: Colors.red),
                      ))),
            ],
          );
        }

        return Wrap(
          spacing: 8,
          children: [
            Chip(
                avatar: const CircleAvatar(
                  child: Icon(Icons.download_rounded),
                ),
                label: Container(
                    alignment: Alignment.centerRight,
                    constraints: const BoxConstraints(
                        minWidth: _minWidth, maxWidth: _minWidth),
                    child: const Text(
                      "0",
                      style: TextStyle(color: Colors.red),
                    ))),
            Chip(
                avatar: const CircleAvatar(
                  child: Icon(Icons.upload_rounded),
                ),
                label: Container(
                    alignment: Alignment.centerRight,
                    constraints: const BoxConstraints(
                        minWidth: _minWidth, maxWidth: _minWidth),
                    child: const Text(
                      "0",
                      style: TextStyle(color: Colors.red),
                    ))),
          ],
        );
      },
    );
  }

  static String _formatBytes(int bytes, int decimals) {
    if (bytes <= 0) return "0B/s";
    const suffixes = [
      "B",
      "KiB",
      "MiB",
      "GiB",
      "TiB",
      "PiB",
      "EiB",
      "ZiB",
      "YiB"
    ];
    final i = (log(bytes) / log(1024)).floor();
    return "${(bytes / pow(1024, i)).toStringAsFixed(decimals)}${suffixes[i]}/s";
  }
}
