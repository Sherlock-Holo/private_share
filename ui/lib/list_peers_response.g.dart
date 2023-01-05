// GENERATED CODE - DO NOT MODIFY BY HAND

part of 'list_peers_response.dart';

// **************************************************************************
// JsonSerializableGenerator
// **************************************************************************

ListPeersResponse _$ListPeersResponseFromJson(Map<String, dynamic> json) =>
    ListPeersResponse(
      (json['peers'] as List<dynamic>)
          .map((e) => ListPeer.fromJson(e as Map<String, dynamic>))
          .toList(),
    );

Map<String, dynamic> _$ListPeersResponseToJson(ListPeersResponse instance) =>
    <String, dynamic>{
      'peers': instance.peers,
    };

ListPeer _$ListPeerFromJson(Map<String, dynamic> json) => ListPeer(
      (json['connected_addrs'] as List<dynamic>)
          .map((e) => e as String)
          .toList(),
      json['peer'] as String,
    );

Map<String, dynamic> _$ListPeerToJson(ListPeer instance) => <String, dynamic>{
      'connected_addrs': instance.connectedAddrs,
      'peer': instance.peer,
    };
