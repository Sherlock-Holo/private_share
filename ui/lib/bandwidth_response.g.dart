// GENERATED CODE - DO NOT MODIFY BY HAND

part of 'bandwidth_response.dart';

// **************************************************************************
// JsonSerializableGenerator
// **************************************************************************

GetBandWidthResponse _$GetBandWidthResponseFromJson(
        Map<String, dynamic> json) =>
    GetBandWidthResponse(
      json['inbound'] as int,
      json['outbound'] as int,
    );

Map<String, dynamic> _$GetBandWidthResponseToJson(
        GetBandWidthResponse instance) =>
    <String, dynamic>{
      'inbound': instance.inbound,
      'outbound': instance.outbound,
    };
