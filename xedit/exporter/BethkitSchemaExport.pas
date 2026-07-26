{******************************************************************************

  This Source Code Form is subject to the terms of the Mozilla Public License,
  v. 2.0. If a copy of the MPL was not distributed with this file, You can obtain
  one at https://mozilla.org/MPL/2.0/.

*******************************************************************************}

unit BethkitSchemaExport;

interface

procedure BethkitWriteSchemaExport(const aGame, aOutputPath: string);

implementation

uses
  Classes,
  SysUtils,
  System.Hash,
  TypInfo,
  Windows,
  wbInterface;

const
{$I BethkitBuildInfo.inc}

type
  TBethkitMapResolver = class
  private
    FSymbols: TStringList;
    function AddressKey(aAddress: NativeUInt): string;
  public
    constructor Create(const aMapPath: string);
    destructor Destroy; override;
    function Resolve(
      aAddress: NativeUInt;
      out aFingerprint, aSymbol, aUnitName: string
    ): Boolean;
    procedure SelfTest;
  end;

  TBethkitSchemaWriter = class
  private
    FCallbacks: TStringList;
    FEncoding: TEncoding;
    FMapResolver: TBethkitMapResolver;
    FNextNodeId: Cardinal;
    FWriter: TStreamWriter;
    function JsonString(const aValue: string): string;
    function NodeName(const aDef: IwbDef): string;
    function NodePath(
      const aParentPath: string;
      const aDef: IwbDef;
      aIndex: Integer
    ): string;
    procedure AddCallback(
      const aPath: string;
      const aCallback: TwbBethkitCallbackInfo
    );
    procedure InspectCallbacks(const aDef: IwbDef; const aPath: string);
    procedure WriteArray(const aDef: IwbArrayDef; const aPath: string);
    procedure WriteCallbacks;
    procedure WriteChoice(
      const aDef: IwbSubRecordUnionDef;
      const aPath: string
    );
    procedure WriteCustom(const aDef: IwbDef; const aPath: string);
    procedure WriteFloat(const aDef: IwbFloatDef; const aPath: string);
    procedure WriteInteger(const aDef: IwbIntegerDef; const aPath: string);
    procedure WriteIntegerType(aIntType: TwbIntType);
    procedure WriteNode(
      const aDef: IwbDef;
      const aPath: string;
      const aName: string
    );
    procedure WriteNodeHeader(
      const aDef: IwbDef;
      const aPath: string;
      const aName: string
    );
    procedure WritePrimitiveBytes(aLength: Integer);
    procedure WriteRecord(const aDef: IwbMainRecordDef);
    procedure WriteRecordMember(
      const aDef: IwbRecordMemberDef;
      const aPath: string;
      aIndex: Integer
    );
    procedure WriteRecordMembers(
      const aDef: IwbRecordDef;
      const aPath: string
    );
    procedure WriteSignature(const aSignature: TwbSignature);
    procedure WriteString(const aDef: IwbStringDef);
    procedure WriteStruct(const aDef: IwbStructDef; const aPath: string);
    procedure WriteSubrecord(
      const aDef: IwbSubRecordDef;
      const aPath: string
    );
  public
    constructor Create(const aOutputPath: string);
    destructor Destroy; override;
    procedure WriteExport(const aGame: string);
  end;

function TBethkitMapResolver.AddressKey(aAddress: NativeUInt): string;
var
  lBase: NativeUInt;
begin
  lBase := NativeUInt(GetModuleHandle(nil));
  if aAddress < lBase + $1000 then
    raise Exception.CreateFmt(
      'Callback address %.8x precedes the executable code segment',
      [aAddress]
    );
  Result := IntToHex(aAddress - lBase - $1000, 8);
end;

constructor TBethkitMapResolver.Create(const aMapPath: string);
var
  lAddress: UInt64;
  lAddressText: string;
  lInPublicsByValue: Boolean;
  lIndex: Integer;
  lLine: string;
  lLines: TStringList;
  lSymbol: string;
begin
  inherited Create;
  if not FileExists(aMapPath) then
    raise Exception.Create('Detailed Delphi MAP file not found: ' + aMapPath);

  FSymbols := TStringList.Create;
  FSymbols.NameValueSeparator := '=';
  FSymbols.Sorted := True;
  FSymbols.Duplicates := dupIgnore;
  lLines := TStringList.Create;
  try
    lLines.LoadFromFile(aMapPath, TEncoding.UTF8);
    lInPublicsByValue := False;
    for lIndex := 0 to Pred(lLines.Count) do begin
      lLine := Trim(lLines[lIndex]);
      if Pos('Address', lLine) = 1 then begin
        lInPublicsByValue := Pos('Publics by Value', lLine) > 0;
        Continue;
      end;
      if not lInPublicsByValue then
        Continue;
      if Pos('Program entry point', lLine) = 1 then
        Break;
      if (Length(lLine) < 15) or (Copy(lLine, 1, 5) <> '0001:') then
        Continue;
      lAddressText := Copy(lLine, 6, 8);
      if not TryStrToUInt64('$' + lAddressText, lAddress) then
        Continue;
      lSymbol := Trim(Copy(lLine, 15, MaxInt));
      if lSymbol <> '' then
        FSymbols.Values[UpperCase(lAddressText)] := lSymbol;
    end;
  finally
    lLines.Free;
  end;
  if FSymbols.Count = 0 then
    raise Exception.Create(
      'Detailed Delphi MAP file contains no value-sorted symbols'
    );
end;

destructor TBethkitMapResolver.Destroy;
begin
  FSymbols.Free;
  inherited;
end;

function TBethkitMapResolver.Resolve(
  aAddress: NativeUInt;
  out aFingerprint, aSymbol, aUnitName: string
): Boolean;
var
  lSeparator: Integer;
  lSource: string;
begin
  aFingerprint := '';
  aSymbol := '';
  aUnitName := '';
  if aAddress = 0 then
    Exit(False);

  aSymbol := FSymbols.Values[AddressKey(aAddress)];
  Result := aSymbol <> '';
  if not Result then
    Exit;

  lSeparator := Pos('.', aSymbol);
  if lSeparator > 1 then
    aUnitName := Copy(aSymbol, 1, Pred(lSeparator));
  lSource :=
    BETHKIT_XEDIT_SOURCE_COMMIT + '|' +
    BETHKIT_EXPORTER_BUILD_SHA256 + '|' +
    AddressKey(aAddress) + '|' +
    aSymbol;
  aFingerprint := LowerCase(
    THashSHA2.GetHashString(
      lSource,
      THashSHA2.TSHA2Version.SHA256
    )
  );
end;

procedure TBethkitMapResolver.SelfTest;
var
  lFingerprint: string;
  lSymbol: string;
  lUnitName: string;
begin
  if not Resolve(
    NativeUInt(@BethkitWriteSchemaExport),
    lFingerprint,
    lSymbol,
    lUnitName
  ) then
    raise Exception.Create(
      'Detailed Delphi MAP file cannot resolve the exporter self-test symbol'
    );
  if Pos('BethkitWriteSchemaExport', lSymbol) = 0 then
    raise Exception.Create(
      'Detailed Delphi MAP file resolved the wrong exporter self-test symbol'
    );
end;

function TBethkitSchemaWriter.JsonString(const aValue: string): string;
var
  lBuilder: TStringBuilder;
  lCharacter: Char;
  lIndex: Integer;
begin
  lBuilder := TStringBuilder.Create(Length(aValue) + 2);
  try
    lBuilder.Append('"');
    for lIndex := 1 to Length(aValue) do begin
      lCharacter := aValue[lIndex];
      case lCharacter of
        '"': lBuilder.Append('\"');
        '\': lBuilder.Append('\\');
        #8: lBuilder.Append('\b');
        #9: lBuilder.Append('\t');
        #10: lBuilder.Append('\n');
        #12: lBuilder.Append('\f');
        #13: lBuilder.Append('\r');
      else
        if Ord(lCharacter) < 32 then
          lBuilder.Append('\u' + IntToHex(Ord(lCharacter), 4))
        else
          lBuilder.Append(lCharacter);
      end;
    end;
    lBuilder.Append('"');
    Result := lBuilder.ToString;
  finally
    lBuilder.Free;
  end;
end;

function TBethkitSchemaWriter.NodeName(const aDef: IwbDef): string;
var
  lNamedDef: IwbNamedDef;
begin
  if Supports(aDef, IwbNamedDef, lNamedDef) then
    Result := lNamedDef.Name
  else
    Result := aDef.DefTypeName;
  if Result = '' then
    Result := aDef.DefTypeName;
end;

function TBethkitSchemaWriter.NodePath(
  const aParentPath: string;
  const aDef: IwbDef;
  aIndex: Integer
): string;
begin
  Result := aParentPath + '/' + IntToStr(aIndex) + ':' + NodeName(aDef);
end;

procedure TBethkitSchemaWriter.AddCallback(
  const aPath: string;
  const aCallback: TwbBethkitCallbackInfo
);
var
  lFingerprint: string;
  lSymbol: string;
  lUnitName: string;
  lValue: string;
begin
  if not FMapResolver.Resolve(
    aCallback.ImplementationAddress,
    lFingerprint,
    lSymbol,
    lUnitName
  ) then begin
    if aCallback.Semantic then
      raise Exception.CreateFmt(
        'Cannot resolve semantic callback %s at %s (address %.8x)',
        [aCallback.Id, aPath, aCallback.ImplementationAddress]
      );
    lSymbol := '';
    lUnitName := '';
    lFingerprint := LowerCase(
      THashSHA2.GetHashString(
        'ui|' + aCallback.Id,
        THashSHA2.TSHA2Version.SHA256
      )
    );
  end;
  lValue :=
    '{"path":' + JsonString(aPath) +
    ',"callback_id":' + JsonString(aCallback.Id) +
    ',"callback_slot":';
  if aCallback.Slot >= 0 then
    lValue := lValue + IntToStr(aCallback.Slot)
  else
    lValue := lValue + 'null';
  lValue := lValue + ',"semantic":';
  if aCallback.Semantic then
    lValue := lValue + 'true'
  else
    lValue := lValue + 'false';
  lValue :=
    lValue +
    ',"implementation_fingerprint":' + JsonString(lFingerprint) +
    ',"implementation_symbol":' + JsonString(lSymbol) +
    ',"implementation_unit":' + JsonString(lUnitName) +
    ',"implementation_source_line":null';
  FCallbacks.Add(lValue + '}');
end;

procedure TBethkitSchemaWriter.InspectCallbacks(
  const aDef: IwbDef;
  const aPath: string
);
var
  lCallbacks: TwbBethkitCallbackInfos;
  lIndex: Integer;
begin
  lCallbacks := aDef.GetBethkitCallbacks;
  for lIndex := Low(lCallbacks) to High(lCallbacks) do
    AddCallback(aPath, lCallbacks[lIndex]);
end;

procedure TBethkitSchemaWriter.WriteArray(
  const aDef: IwbArrayDef;
  const aPath: string
);
var
  lElementPath: string;
begin
  if Assigned(aDef.CountCallBack) or
     (aDef.CountPath <> '') or
     (aDef.ElementCount <= 0) or
     aDef.Terminated
  then begin
    WriteCustom(aDef, aPath);
    Exit;
  end;

  FWriter.Write(',"kind":"array","element":');
  lElementPath := aPath + '/element';
  WriteNode(aDef.Element, lElementPath, NodeName(aDef.Element));
  FWriter.Write(',"count":{"kind":"fixed","count":');
  FWriter.Write(aDef.ElementCount);
  FWriter.Write('}');
end;

procedure TBethkitSchemaWriter.WriteCallbacks;
var
  lIndex: Integer;
begin
  FWriter.Write(',"callbacks":[');
  for lIndex := 0 to Pred(FCallbacks.Count) do begin
    if lIndex > 0 then
      FWriter.Write(',');
    FWriter.Write(FCallbacks[lIndex]);
  end;
  FWriter.Write(']');
end;

procedure TBethkitSchemaWriter.WriteChoice(
  const aDef: IwbSubRecordUnionDef;
  const aPath: string
);
var
  lIndex: Integer;
begin
  FWriter.Write(',"kind":"choice","alternatives":[');
  for lIndex := 0 to Pred(aDef.MemberCount) do begin
    if lIndex > 0 then
      FWriter.Write(',');
    WriteRecordMember(aDef.Members[lIndex], aPath, lIndex);
  end;
  FWriter.Write(']');
end;

procedure TBethkitSchemaWriter.WriteCustom(
  const aDef: IwbDef;
  const aPath: string
);
var
  lDecoder: string;
begin
  lDecoder := 'xedit.' + LowerCase(
    GetEnumName(TypeInfo(TwbDefType), Ord(aDef.DefType))
  );
  AddCallback(aPath, 'decoder.required', True);
  FWriter.Write(',"kind":"custom","decoder":');
  FWriter.Write(JsonString(lDecoder));
  FWriter.Write(',"configuration":{"xedit_def_type":');
  FWriter.Write(JsonString(aDef.DefTypeName));
  FWriter.Write('}');
end;

procedure TBethkitSchemaWriter.WriteFloat(
  const aDef: IwbFloatDef;
  const aPath: string
);
var
  lWidth: Integer;
begin
  if aDef.Terminator then begin
    WriteCustom(aDef, aPath);
    Exit;
  end;
  case aDef.Kind of
    fkHalf: lWidth := 2;
    fkSingle: lWidth := 4;
    fkDouble: lWidth := 8;
  else
    lWidth := 0;
  end;
  FWriter.Write(
    ',"kind":"primitive","primitive":{"type":"float","width":'
  );
  FWriter.Write(lWidth);
  FWriter.Write(',"byte_order":"little_endian"}');
end;

procedure TBethkitSchemaWriter.WriteInteger(
  const aDef: IwbIntegerDef;
  const aPath: string
);
var
  lFlagsDef: IwbFlagsDef;
  lFormIdChecked: IwbFormIDChecked;
  lFormIdDef: IwbFormID;
  lFormatter: IwbIntegerDefFormater;
  lIndex: Integer;
begin
  if aDef.Terminator then begin
    WriteCustom(aDef, aPath);
    Exit;
  end;
  if aDef.IntType in [it0, itU24, itU6to30] then begin
    WriteCustom(aDef, aPath);
    Exit;
  end;
  if aDef.FormaterCanChange then begin
    WriteCustom(aDef, aPath);
    Exit;
  end;

  lFormatter := aDef.Formater[nil];
  if Supports(lFormatter, IwbCallbackDef) or
     Supports(lFormatter, IwbIntegerDefFormaterUnion)
  then begin
    AddCallback(aPath, 'integer.formatter', True);
    WriteCustom(aDef, aPath);
    Exit;
  end;
  if Supports(lFormatter, IwbFormID, lFormIdDef) then begin
    FWriter.Write(
      ',"kind":"primitive","primitive":{"type":"form_id","targets":['
    );
    if Supports(lFormatter, IwbFormIDChecked, lFormIdChecked) then
      for lIndex := 0 to Pred(lFormIdChecked.SignatureCount) do begin
        if lIndex > 0 then
          FWriter.Write(',');
        WriteSignature(lFormIdChecked.Signatures[lIndex]);
      end;
    FWriter.Write(']}');
    Exit;
  end;

  if Supports(lFormatter, IwbFlagsDef, lFlagsDef) then begin
    FWriter.Write(
      ',"kind":"primitive","primitive":{"type":"flags","integer":'
    );
    WriteIntegerType(aDef.IntType);
    FWriter.Write(',"bits":[');
    for lIndex := 0 to Pred(lFlagsDef.FlagCount) do begin
      if lIndex > 0 then
        FWriter.Write(',');
      FWriter.Write('[');
      FWriter.Write(lIndex);
      FWriter.Write(',');
      FWriter.Write(JsonString(lFlagsDef.Flags[lIndex, False]));
      FWriter.Write(']');
    end;
    FWriter.Write(']}');
    Exit;
  end;

  if Supports(lFormatter, IwbEnumDef) then begin
    WriteCustom(aDef, aPath);
    Exit;
  end;

  FWriter.Write(
    ',"kind":"primitive","primitive":{"type":"integer","integer":'
  );
  WriteIntegerType(aDef.IntType);
  FWriter.Write('}');
end;

procedure TBethkitSchemaWriter.WriteIntegerType(aIntType: TwbIntType);
var
  lSigned: Boolean;
  lWidth: Integer;
begin
  lSigned := aIntType in [itS8, itS16, itS32, itS64];
  case aIntType of
    itU8, itS8: lWidth := 1;
    itU16, itS16: lWidth := 2;
    itU32, itS32: lWidth := 4;
    itU64, itS64: lWidth := 8;
  else
    lWidth := 0;
  end;
  FWriter.Write('{"width":');
  FWriter.Write(lWidth);
  FWriter.Write(',"signed":');
  if lSigned then
    FWriter.Write('true')
  else
    FWriter.Write('false');
  FWriter.Write(',"byte_order":"little_endian"}');
end;

procedure TBethkitSchemaWriter.WriteNode(
  const aDef: IwbDef;
  const aPath: string;
  const aName: string
);
var
  lArrayDef: IwbArrayDef;
  lFloatDef: IwbFloatDef;
  lIntegerDef: IwbIntegerDef;
  lRecordDef: IwbRecordDef;
  lStringDef: IwbStringDef;
  lStructDef: IwbStructDef;
  lSubRecordDef: IwbSubRecordDef;
  lSubRecordUnionDef: IwbSubRecordUnionDef;
begin
  WriteNodeHeader(aDef, aPath, aName);
  InspectCallbacks(aDef, aPath);

  case aDef.DefType of
    dtRecord,
    dtSubRecordStruct: begin
      if Supports(aDef, IwbRecordDef, lRecordDef) then begin
        FWriter.Write(',"kind":"sequence","children":[');
        WriteRecordMembers(lRecordDef, aPath);
        FWriter.Write(']');
      end else
        WriteCustom(aDef, aPath);
    end;
    dtSubRecord: begin
      if Supports(aDef, IwbSubRecordDef, lSubRecordDef) then
        WriteSubrecord(lSubRecordDef, aPath)
      else
        WriteCustom(aDef, aPath);
    end;
    dtSubRecordArray: begin
      FWriter.Write(
        ',"kind":"repeat","minimum":0,"maximum":null,"child":'
      );
      WriteRecordMember(
        (aDef as IwbSubRecordArrayDef).Element,
        aPath + '/repeat',
        0
      );
    end;
    dtSubRecordUnion: begin
      if Supports(aDef, IwbSubRecordUnionDef, lSubRecordUnionDef) then
        WriteChoice(lSubRecordUnionDef, aPath)
      else
        WriteCustom(aDef, aPath);
    end;
    dtString: begin
      if Supports(aDef, IwbStringDef, lStringDef) then
        WriteString(lStringDef)
      else
        WriteCustom(aDef, aPath);
    end;
    dtInteger: begin
      if Supports(aDef, IwbIntegerDef, lIntegerDef) then
        WriteInteger(lIntegerDef, aPath)
      else
        WriteCustom(aDef, aPath);
    end;
    dtFloat: begin
      if Supports(aDef, IwbFloatDef, lFloatDef) then
        WriteFloat(lFloatDef, aPath)
      else
        WriteCustom(aDef, aPath);
    end;
    dtArray: begin
      if Supports(aDef, IwbArrayDef, lArrayDef) then
        WriteArray(lArrayDef, aPath)
      else
        WriteCustom(aDef, aPath);
    end;
    dtStruct: begin
      if Supports(aDef, IwbStructDef, lStructDef) and
         not Supports(aDef, IwbStructCDef)
      then
        WriteStruct(lStructDef, aPath)
      else
        WriteCustom(aDef, aPath);
    end;
    dtByteArray: WritePrimitiveBytes(-1);
    dtGuid: WritePrimitiveBytes(16);
    dtEmpty: WritePrimitiveBytes(0);
  else
    WriteCustom(aDef, aPath);
  end;
  FWriter.Write('}');
end;

procedure TBethkitSchemaWriter.WriteNodeHeader(
  const aDef: IwbDef;
  const aPath: string;
  const aName: string
);
begin
  FWriter.Write('{"id":');
  FWriter.Write(FNextNodeId);
  Inc(FNextNodeId);
  FWriter.Write(',"path":');
  FWriter.Write(JsonString(aPath));
  FWriter.Write(',"name":');
  FWriter.Write(JsonString(aName));
  FWriter.Write(',"required":');
  if aDef.Required then
    FWriter.Write('true')
  else
    FWriter.Write('false');
  FWriter.Write(',"condition":null');
end;

procedure TBethkitSchemaWriter.WritePrimitiveBytes(aLength: Integer);
begin
  FWriter.Write(
    ',"kind":"primitive","primitive":{"type":"bytes","length":'
  );
  if aLength < 0 then
    FWriter.Write('null')
  else
    FWriter.Write(aLength);
  FWriter.Write('}');
end;

procedure TBethkitSchemaWriter.WriteRecord(const aDef: IwbMainRecordDef);
var
  lPath: string;
begin
  lPath := aDef.DefaultSignature;
  FWriter.Write('{"signature":');
  WriteSignature(aDef.DefaultSignature);
  FWriter.Write(',"name":');
  FWriter.Write(JsonString(aDef.Name));
  FWriter.Write(',"root":');
  WriteNode(aDef, lPath, aDef.Name);
  FWriter.Write('}');
end;

procedure TBethkitSchemaWriter.WriteRecordMember(
  const aDef: IwbRecordMemberDef;
  const aPath: string;
  aIndex: Integer
);
var
  lPath: string;
begin
  lPath := NodePath(aPath, aDef, aIndex);
  WriteNode(aDef, lPath, NodeName(aDef));
end;

procedure TBethkitSchemaWriter.WriteRecordMembers(
  const aDef: IwbRecordDef;
  const aPath: string
);
var
  lIndex: Integer;
begin
  for lIndex := 0 to Pred(aDef.MemberCount) do begin
    if lIndex > 0 then
      FWriter.Write(',');
    WriteRecordMember(aDef.Members[lIndex], aPath, lIndex);
  end;
end;

procedure TBethkitSchemaWriter.WriteSignature(
  const aSignature: TwbSignature
);
var
  lIndex: Integer;
begin
  FWriter.Write('[');
  for lIndex := 0 to 3 do begin
    if lIndex > 0 then
      FWriter.Write(',');
    if lIndex < Length(aSignature) then
      FWriter.Write(Ord(aSignature[lIndex]))
    else
      FWriter.Write(0);
  end;
  FWriter.Write(']');
end;

procedure TBethkitSchemaWriter.WriteString(const aDef: IwbStringDef);
begin
  FWriter.Write(
    ',"kind":"primitive","primitive":{"type":"string","string":' +
    '{"encoding":"windows_1252","zero_terminated":'
  );
  if aDef.Terminator then
    FWriter.Write('true')
  else
    FWriter.Write('false');
  FWriter.Write(
    ',' +
    '"fixed_length":'
  );
  if aDef.StringSize > 0 then
    FWriter.Write(aDef.StringSize)
  else
    FWriter.Write('null');
  FWriter.Write('}}');
end;

procedure TBethkitSchemaWriter.WriteStruct(
  const aDef: IwbStructDef;
  const aPath: string
);
var
  lIndex: Integer;
  lMember: IwbValueDef;
begin
  FWriter.Write(',"kind":"struct","fields":[');
  for lIndex := 0 to Pred(aDef.MemberCount) do begin
    if lIndex > 0 then
      FWriter.Write(',');
    lMember := aDef.Members[lIndex];
    WriteNode(
      lMember,
      NodePath(aPath, lMember, lIndex),
      NodeName(lMember)
    );
  end;
  FWriter.Write(']');
end;

procedure TBethkitSchemaWriter.WriteSubrecord(
  const aDef: IwbSubRecordDef;
  const aPath: string
);
var
  lPayloadPath: string;
begin
  if (aDef.SignatureCount <> 1) or aDef.HasUnusedData then begin
    WriteCustom(aDef, aPath);
    Exit;
  end;
  FWriter.Write(',"kind":"subrecord","signature":');
  WriteSignature(aDef.DefaultSignature);
  FWriter.Write(',"payload":');
  lPayloadPath := aPath + '/payload';
  WriteNode(aDef.Value, lPayloadPath, NodeName(aDef.Value));
end;

constructor TBethkitSchemaWriter.Create(const aOutputPath: string);
begin
  inherited Create;
  FCallbacks := TStringList.Create;
  FCallbacks.Sorted := True;
  FCallbacks.Duplicates := dupIgnore;
  FNextNodeId := 0;
  FEncoding := TUTF8Encoding.Create(False);
  FMapResolver := TBethkitMapResolver.Create(
    ChangeFileExt(ParamStr(0), '.map')
  );
  FMapResolver.SelfTest;
  FWriter := TStreamWriter.Create(aOutputPath, False, FEncoding);
  FWriter.NewLine := #10;
end;

destructor TBethkitSchemaWriter.Destroy;
begin
  FWriter.Free;
  FMapResolver.Free;
  FEncoding.Free;
  FCallbacks.Free;
  inherited;
end;

procedure TBethkitSchemaWriter.WriteExport(const aGame: string);
var
  lExecutableHash: string;
  lIndex: Integer;
  lMapHash: string;
begin
  wbInitRecords;
  lExecutableHash := LowerCase(
    THashSHA2.GetHashStringFromFile(ParamStr(0), THashSHA2.TSHA2Version.SHA256)
  );
  lMapHash := LowerCase(
    THashSHA2.GetHashStringFromFile(
      ChangeFileExt(ParamStr(0), '.map'),
      THashSHA2.TSHA2Version.SHA256
    )
  );

  FWriter.Write('{"contract_version":2,"provenance":{');
  FWriter.Write('"source_tag":' + JsonString(BETHKIT_XEDIT_SOURCE_TAG));
  FWriter.Write(
    ',"source_commit":' + JsonString(BETHKIT_XEDIT_SOURCE_COMMIT)
  );
  FWriter.Write(
    ',"source_archive_sha256":' +
    JsonString(BETHKIT_XEDIT_SOURCE_ARCHIVE_SHA256)
  );
  FWriter.Write(
    ',"exporter_version":' + JsonString(BETHKIT_EXPORTER_VERSION)
  );
  FWriter.Write(
    ',"exporter_binary_sha256":' + JsonString(lExecutableHash)
  );
  FWriter.Write(
    ',"exporter_map_sha256":' + JsonString(lMapHash)
  );
  FWriter.Write(
    ',"exporter_patch_sha256":' +
    JsonString(BETHKIT_EXPORTER_PATCH_SHA256)
  );
  FWriter.Write(
    ',"exporter_build_sha256":' +
    JsonString(BETHKIT_EXPORTER_BUILD_SHA256)
  );
  FWriter.Write('},"game":' + JsonString(aGame) + ',"records":[');

  for lIndex := Low(wbRecordDefs) to High(wbRecordDefs) do begin
    if lIndex > Low(wbRecordDefs) then
      FWriter.Write(',');
    WriteRecord(wbRecordDefs[lIndex].rdeDef);
  end;
  FWriter.Write(']');
  WriteCallbacks;
  FWriter.Write('}');
  FWriter.Flush;
end;

procedure BethkitWriteSchemaExport(const aGame, aOutputPath: string);
var
  lWriter: TBethkitSchemaWriter;
begin
  lWriter := TBethkitSchemaWriter.Create(aOutputPath);
  try
    lWriter.WriteExport(aGame);
  finally
    lWriter.Free;
  end;
end;

end.
