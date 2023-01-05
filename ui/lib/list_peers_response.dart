import 'package:json_annotation/json_annotation.dart';

part 'list_peers_response.g.dart';

@JsonSerializable()
class ListPeersResponse {
  final List<ListPeer> peers;

  ListPeersResponse(this.peers);

  factory ListPeersResponse.fromJson(Map<String, dynamic> json) =>
      _$ListPeersResponseFromJson(json);

  Map<String, dynamic> toJson() => _$ListPeersResponseToJson(this);
}

@JsonSerializable()
class ListPeer {
  @JsonKey(name: 'connected_addrs')
  final List<String> connectedAddrs;

  final String peer;

  ListPeer(this.connectedAddrs, this.peer);

  factory ListPeer.fromJson(Map<String, dynamic> json) =>
      _$ListPeerFromJson(json);

  Map<String, dynamic> toJson() => _$ListPeerToJson(this);
}
