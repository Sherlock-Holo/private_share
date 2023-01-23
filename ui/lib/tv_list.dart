import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:http/http.dart' as http;
import 'package:ui/util.dart';
import 'package:web_socket_channel/web_socket_channel.dart';

import 'list_tv_response.dart';

class TvList extends StatefulWidget {
  final String filename;

  const TvList({super.key, required this.filename});

  @override
  State<StatefulWidget> createState() => _TvListState();
}

class _TvListState extends State<TvList> {
  late final WebSocketChannel _webSocketChannel;
  final Map<String, ListTVResponse> _tvs = {};

  @override
  void initState() {
    super.initState();

    final url = Util.getWsUri("/api/list_tv", query: {"timeout": "10000"});

    _webSocketChannel = WebSocketChannel.connect(Uri.parse(url));
  }

  @override
  void dispose() {
    _webSocketChannel.sink.close();

    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text("Video List"),
      content: Container(
        constraints: const BoxConstraints(maxHeight: 500, minWidth: 300),
        child: _streamingBuildTVList(),
      ),
      actions: [
        TextButton(
            onPressed: () {
              setState(() {
                _refreshTVList();
              });
            },
            child: Column(
              children: const [
                Icon(Icons.refresh_rounded),
                Padding(
                  padding: EdgeInsets.symmetric(vertical: 2.0),
                ),
                Text("Refresh")
              ],
            )),
        TextButton(
            onPressed: () {
              Navigator.of(context).pop();
            },
            child: Column(
              children: const [
                Icon(Icons.close_rounded),
                Padding(
                  padding: EdgeInsets.symmetric(vertical: 2.0),
                ),
                Text("Close")
              ],
            ))
      ],
    );
  }

  void _refreshTVList() {
    _webSocketChannel.sink.add(jsonEncode({"timeout": 10000}));

    _tvs.clear();
  }

  Widget _streamingBuildTVList() {
    return StreamBuilder(
      stream: _webSocketChannel.stream,
      builder: (context, snapshot) {
        if (snapshot.hasData) {
          final json =
              jsonDecode(snapshot.data!.toString()) as Map<String, dynamic>;
          final tv = ListTVResponse.fromJson(json);
          _tvs[tv.encodedUrl] = tv;
        } else if (snapshot.hasError) {
          return Text(
            snapshot.error.toString(),
            style: const TextStyle(color: Colors.red),
          );
        }

        return _buildTVList();
      },
    );
  }

  Widget _buildTVList() {
    final tvList = _tvs.values.toList();
    tvList.sort((a, b) => a.friendName.compareTo(b.friendName));

    return ListView.builder(
      itemCount: tvList.length,
      itemBuilder: (context, index) {
        final tv = tvList[index];

        return _buildTV(tv);
      },
    );
  }

  Widget _buildTV(ListTVResponse tv) {
    return Card(
      child: ListTile(
        leading: const Icon(Icons.tv_rounded),
        title: Text(tv.friendName),
        onTap: () {
          _playVideo(tv).then((value) {
            if (value != 200) {
              ScaffoldMessenger.of(context).showSnackBar(SnackBar(
                  content: Text("play video failed $value}",
                      style: const TextStyle(color: Colors.red))));
            } else {
              ScaffoldMessenger.of(context).showSnackBar(const SnackBar(
                  content: Text(
                "play video done",
              )));

              Navigator.of(context).pop();
            }
          });
        },
      ),
    );
  }

  Future<int> _playVideo(ListTVResponse tv) async {
    final url = Util.getUri("/api/play_tv/${tv.encodedUrl}/${widget.filename}");
    final resp = await http.post(url);

    return resp.statusCode;
  }
}
