import 'dart:convert';

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

          return Column(
            children: [
              ListTile(
                leading: const Icon(Icons.download_rounded),
                title: Text("$inboundSpeed"),
              ),
              ListTile(
                leading: const Icon(Icons.upload_rounded),
                title: Text("$outboundSpeed"),
              ),
            ],
          );
        }

        if (snapshot.hasError) {
          final err = snapshot.error!;

          return Column(
            children: [
              ListTile(
                leading: const Icon(Icons.download_rounded),
                title: Text(
                  "$err",
                  style: const TextStyle(color: Colors.red),
                ),
              ),
              ListTile(
                leading: const Icon(Icons.upload_rounded),
                title: Text(
                  "$err",
                  style: const TextStyle(color: Colors.red),
                ),
              ),
            ],
          );
        }

        return Column(
          children: const [
            ListTile(
              leading: Icon(Icons.download_rounded),
              title: Text("0"),
            ),
            ListTile(
              leading: Icon(Icons.upload_rounded),
              title: Text("0"),
            ),
          ],
        );
      },
    );
  }
}
