require "file_utils"
require "csv"
require "json"
require "http/client"
require "uri"
require "db"
require "sqlite3"

module SheetParser
  SHEET_ID     = "1Kc9aME3xlUC_vV5dFRe457OchqUOrwuiX_pQykjCF68"
  SHEET_FOLDER = "SheetData"
  DATA_FOLDER  = "Data"

  PARTS_V2_SHEET_GID = "319672878"
  CORES_SHEET_GID    = "911413911"

  PARTS_V2_SHEET = "#{SHEET_FOLDER}/parts2.csv"
  CORES_SHEET    = "#{SHEET_FOLDER}/cores.csv"

  OUTPUT_FILE = "#{DATA_FOLDER}/FullData.sqlite3"

  VALID_PART_CATEGORIES = ["AR", "Sniper", "SMG", "LMG", "Shotgun", "BR", "Weird", "Sidearm"]
  VALID_PART_TYPES      = ["Barrels", "Magazines", "Grips", "Stocks"]
  VALID_PRICE_TYPES     = ["Coin", "WC", "Follow", "Robux", "Free", "Spin", "Limited", "Missions", "Verify discord", "Season Pass 1", "Unknown"]

  PART_PROPERTY_NAMES = [
    "Magazine_Size", "Reload_Time", "Damage", "Detection_Radius", "Equip_Time", "Fire_Rate", "Health", "Magazine_Cap", "Movement_Speed", "Pellets", "Range", "Recoil", "Reload_Speed", "Spread",
  ]

  CORE_PROPERTY_NAMES = [
    "Damage", "Dropoff_Studs", "Fire_Rate", "Hipfire_Spread", "ADS_Spread", "Time_To_Aim", "Detection_Radius", "Burst", "Movement_Speed_Modifier", "Suppression", "Health", "Equip_Time", "Recoil_Hip_Horizontal", "Recoil_Hip_Vertical", "Recoil_Aim_Horizontal", "Recoil_Aim_Vertical",
  ]

  CURRENT_CATEGORIES = {
    "Primary"   => {"AR" => 0, "Sniper" => 1, "SMG" => 2, "Shotgun" => 3, "LMG" => 4, "Weird" => 5, "BR" => 6},
    "Secondary" => {"Sidearm" => 7},
  }

  CURRENT_PENALTIES = [
    [1.00, 0.70, 0.75, 0.70, 0.75, 1.00, 0.80, 0.65],
    [0.70, 1.00, 0.60, 0.60, 0.80, 1.00, 0.85, 0.50],
    [0.80, 0.60, 1.00, 0.65, 0.65, 1.00, 0.70, 0.70],
    [0.70, 0.50, 0.65, 1.00, 0.75, 1.00, 0.60, 0.65],
    [0.75, 0.80, 0.65, 0.75, 1.00, 1.00, 0.85, 0.50],
    [1.00, 1.00, 1.00, 1.00, 1.00, 1.00, 1.00, 1.00],
    [0.80, 0.85, 0.70, 0.60, 0.85, 1.00, 1.00, 0.65],
    [0.65, 0.50, 0.75, 0.65, 0.50, 1.00, 0.65, 1.00],
  ]

  alias Primitive = Int32 | Int64 | Float64 | String | Bool | Nil
  alias JsonValue = Primitive | Array(JsonValue) | Hash(String, JsonValue) | Array(Hash(String, JsonValue)) | Hash(String, Hash(String, Int32)) | Array(Array(Float64))
  alias ItemRows = Array(Hash(String, JsonValue))
  alias DataSections = Hash(String, ItemRows)

  struct ExportData
    getter data : DataSections
    getter penalties : Array(Array(Float64))
    getter categories : Hash(String, Hash(String, Int32))

    def initialize(@data : DataSections, @penalties : Array(Array(Float64)), @categories : Hash(String, Hash(String, Int32)))
    end
  end

  class ParseError < Exception
  end

  struct SheetExport
    getter gid : String
    getter output_path : String
    getter url_override : String?

    def initialize(@gid : String, @output_path : String, @url_override : String? = nil)
    end

    def export_url(sheet_id : String)
      url_override || "https://docs.google.com/spreadsheets/d/#{sheet_id}/export?format=csv&id=#{sheet_id}&gid=#{gid}"
    end
  end

  class SheetDownloader
    MAX_REDIRECTS = 5

    def initialize(@sheet_id : String, @sheet_folder : String)
    end

    def download(exports : Array(SheetExport))
      clear_sheet_folder
      exports.each { |export| download_file(export.export_url(@sheet_id), export.output_path) }
    end

    private def clear_sheet_folder
      Dir.mkdir_p(@sheet_folder)
      Dir.each_child(@sheet_folder) do |entry|
        path = File.join(@sheet_folder, entry)
        File.file?(path) ? File.delete(path) : FileUtils.rm_rf(path)
      end
    end

    private def download_file(url : String, output_path : String)
      response = get_with_redirects(url)
      raise "Failed to download #{output_path}. status=#{response.status_code}" unless response.success?

      Dir.mkdir_p(File.dirname(output_path))
      File.write(output_path, response.body)
    end

    private def get_with_redirects(initial_url : String) : HTTP::Client::Response
      url = initial_url
      redirects = 0

      loop do
        response = HTTP::Client.get(url)
        return response unless redirect_status?(response.status_code)

        location = response.headers["Location"]?
        raise "Redirect response missing Location header for #{url}" unless location

        redirects += 1
        raise "Too many redirects while downloading #{initial_url}" if redirects > MAX_REDIRECTS

        url = resolve_redirect_url(url, location)
      end
    end

    private def redirect_status?(status_code : Int32) : Bool
      status_code.in?({301, 302, 303, 307, 308})
    end

    private def resolve_redirect_url(current_url : String, location : String) : String
      return location if location.starts_with?("http://") || location.starts_with?("https://")

      current_uri = URI.parse(current_url)
      base = "#{current_uri.scheme}://#{current_uri.authority}"
      location.starts_with?('/') ? "#{base}#{location}" : "#{base}/#{location}"
    end
  end

  module Normalizer
    extend self

def normalize_numeric_value(raw_value : String, expect_range : Bool = false) : JsonValue
  value = raw_value.strip

  return nil if value.empty? || value == "🎲"

  cleaned = value
    .gsub('°', "")
    .gsub("s", "")
    .gsub("rpm", "")
    .gsub('%', "")
    .gsub(',', "")
    .strip

  if expect_range
    # Open-ended lower/upper range, e.g. "23 >"
    if match = cleaned.match(/\A(.+?)\s*>\s*\z/)
      return [
        coerce_number(parse_single_or_multiplier(match[1].strip)),
        nil,
      ] of JsonValue
    end

    pieces = cleaned.gsub('>', '-').split(" - ")

    raise ParseError.new(
      "Expected numeric range but got: #{raw_value.inspect}"
    ) unless pieces.size == 2

    return pieces
      .map { |piece| coerce_number(parse_single_or_multiplier(piece.strip)) }
      .map { |v| v.as(JsonValue) }
  end

  cleaned = cleaned.gsub('>', '-').strip
  coerce_number(parse_single_or_multiplier(cleaned))
end

    def detect_price_type(price : String) : String
      normalized = price.strip
      return "Coin" if normalized.empty?
      return normalized if VALID_PRICE_TYPES.includes?(normalized)

      capitalized = normalized.capitalize
      return capitalized if VALID_PRICE_TYPES.includes?(capitalized)

      return "WC" if normalized.includes?("WC")
      return "WC" if normalized == "Weird Boxes"
      return "Robux" if normalized == "Exclusive Weird Boxes"

      numeric_candidate = normalized.gsub(',', "")
      return "Coin" if numeric_candidate.to_i?

      "Unknown"
    end

    private def parse_single_or_multiplier(value : String) : Float64
      if value.includes?('x')
        left, right = value.split('x', 2)
        return left.to_f * right.to_f
      end
      value.to_f
    end

    private def coerce_number(value : Float64) : JsonValue
      whole = value.to_i
      whole.to_f == value ? whole : value
    end
  end

  class PartsParser
    @seen_parts = {} of Tuple(String, String) => Set(String)

    def initialize
      VALID_PART_CATEGORIES.each do |category|
        VALID_PART_TYPES.each do |part_type|
          @seen_parts[{category, part_type}] = Set(String).new
        end
      end
    end

    def parse_file(path : String) : Hash(String, Array(Hash(String, JsonValue)))
      rows = SheetParser.read_csv_rows(path)
      truncated = [] of Array(String)
      rows.each do |row|
        break if row.size >= 2 && row[1].starts_with?("Notable ")
        truncated << row
      end
      parse_rows(truncated)
    end

    def parse_rows(rows : Array(Array(String))) : Hash(String, Array(Hash(String, JsonValue)))
      output = {"Barrels" => [] of Hash(String, JsonValue), "Magazines" => [] of Hash(String, JsonValue), "Grips" => [] of Hash(String, JsonValue), "Stocks" => [] of Hash(String, JsonValue)}

      current_category = "AR"
      current_type = ""

      rows.each do |row|
        next if row.empty?
        raise ParseError.new("Invalid parts row length: expected 17, got #{row.size}") unless row.size == 17

        name = row[1].strip
        if divider = parse_divider(name)
          current_category, current_type = divider
          next
        end

        raise ParseError.new("Part encountered before section header") if current_type.empty?

        seen_key = {current_category, current_type}
        raise ParseError.new("Duplicate part name #{name}") if @seen_parts[seen_key].includes?(name)
        @seen_parts[seen_key] << name

        part = {"Price_Type" => Normalizer.detect_price_type(row[0]), "Name" => name, "Category" => current_category} of String => JsonValue

        (2..15).each do |index|
          cell = row[index].strip
          next if cell.empty?
          part[PART_PROPERTY_NAMES[index - 2]] = Normalizer.normalize_numeric_value(SheetParser.extract_leading_token(cell))
        end

        output[current_type] << part
      end

      output
    end

    private def parse_divider(name : String) : Tuple(String, String)?
      parts = name.split(" ")
      return nil unless parts.size == 2
      category, part_type = parts
      return nil unless VALID_PART_CATEGORIES.includes?(category) && VALID_PART_TYPES.includes?(part_type)
      {category, part_type}
    end
  end

  class CoresParser
    CORE_ROW_WIDTH = 18

    def parse_file(path : String) : Array(Hash(String, JsonValue))
      parse_rows(SheetParser.read_csv_rows(path))
    end

    def parse_rows(rows : Array(Array(String))) : Array(Hash(String, JsonValue))
      output = [] of Hash(String, JsonValue)
      current_category = "AR"

      rows.each do |row|
        next if row.empty?
        raise ParseError.new("Invalid cores row length: expected at least #{CORE_ROW_WIDTH}, got #{row.size}") if row.size < CORE_ROW_WIDTH
        trimmed_row = row.first(CORE_ROW_WIDTH)

        name = trimmed_row[1].strip
        if category_divider?(name)
          current_category = name[0...-6]
          next
        end

        core = {"Price_Type" => Normalizer.detect_price_type(trimmed_row[0]), "Name" => name, "Category" => current_category} of String => JsonValue

        (2..17).each do |index|
          cell = trimmed_row[index]
          if index == 2
            pellets = extract_pellets(cell)
            core["Pellets"] = pellets if pellets
          end
          value = Normalizer.normalize_numeric_value(cell, expect_range: index <= 3 || index >= 14)
          core[CORE_PROPERTY_NAMES[index - 2]] = value unless value.nil?
        end

        output << core
      end

      output
    end

    private def category_divider?(name : String) : Bool
      name.ends_with?(" Cores") && VALID_PART_CATEGORIES.includes?(name[0...-6])
    end

    private def extract_pellets(cell : String) : Int32?
      first_segment = cell.split(" > ")[0]
      pieces = first_segment.split('x')
      return nil unless pieces.size == 2
      pieces[1].to_i?
    end
  end

  extend self

  def read_csv_rows(path : String, skip_header_rows : Int32 = 2, trim_first_column : Bool = true) : Array(Array(String))
    rows = CSV.parse(File.read(path)).map(&.to_a)
    data_rows = rows[skip_header_rows..] || [] of Array(String)
    return data_rows.map { |row| row[1..] || [] of String } if trim_first_column
    data_rows
  end

  def extract_leading_token(cell : String) : String
    pieces = cell.split(" ", 2)
    raise ParseError.new("Invalid property cell format: #{cell.inspect}") if pieces.size < 2
    pieces[0]
  end

  def save_json(data : JsonValue, output_path : String)
    Dir.mkdir_p(File.dirname(output_path))
    File.write(output_path, data.to_json)
  end

  def save_sqlite(export_data : ExportData, output_path : String)
    Dir.mkdir_p(File.dirname(output_path))
    File.delete(output_path) if File.exists?(output_path)

    DB.open("sqlite3://#{output_path}") do |db|
      create_schema(db)
      insert_categories(db, export_data.categories)
      insert_penalties(db, export_data.penalties)

      records = export_data.data
      insert_cores(db, records["Cores"])
      insert_magazines(db, records["Magazines"])
      insert_parts(db, "Barrels", records["Barrels"])
      insert_parts(db, "Grips", records["Grips"])
      insert_parts(db, "Stocks", records["Stocks"])
    end
  end

  private def create_schema(db : DB::Database)
    db.exec <<-SQL
      CREATE TABLE categories (
        name TEXT PRIMARY KEY,
        idx INTEGER NOT NULL
      );
    SQL

    db.exec <<-SQL
      CREATE TABLE penalties (
        core_idx INTEGER NOT NULL,
        part_idx INTEGER NOT NULL,
        value REAL NOT NULL,
        PRIMARY KEY (core_idx, part_idx)
      );
    SQL

    db.exec <<-SQL
      CREATE TABLE cores (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL,
        category TEXT NOT NULL,
        damage REAL NOT NULL,
        damage_end REAL NOT NULL,
        fire_rate REAL NOT NULL
      );
    SQL

    db.exec <<-SQL
      CREATE TABLE magazines (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL,
        category TEXT NOT NULL,
        magazine_size REAL NOT NULL,
        reload_time REAL NOT NULL,
        damage_mod REAL NOT NULL,
        fire_rate_mod REAL NOT NULL
      );
    SQL

    db.exec <<-SQL
      CREATE TABLE parts (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        part_type TEXT NOT NULL,
        name TEXT NOT NULL,
        category TEXT NOT NULL,
        damage_mod REAL NOT NULL,
        fire_rate_mod REAL NOT NULL
      );
    SQL
  end

  private def insert_categories(db : DB::Database, categories : Hash(String, Hash(String, Int32)))
    categories.each_value do |group|
      group.each do |name, idx|
        db.exec("INSERT INTO categories (name, idx) VALUES (?, ?)", name, idx)
      end
    end
  end

  private def insert_penalties(db : DB::Database, penalties : Array(Array(Float64)))
    penalties.each_with_index do |row, core_idx|
      row.each_with_index do |value, part_idx|
        db.exec("INSERT INTO penalties (core_idx, part_idx, value) VALUES (?, ?, ?)", core_idx, part_idx, value)
      end
    end
  end

  private def insert_cores(db : DB::Database, cores : Array(Hash(String, JsonValue)))
    cores.each do |core|
      damage, damage_end = extract_damage_pair(core["Damage"]?)
      db.exec(
        "INSERT INTO cores (name, category, damage, damage_end, fire_rate) VALUES (?, ?, ?, ?, ?)",
        core["Name"].as(String),
        core["Category"].as(String),
        damage,
        damage_end,
        json_value_to_f(core["Fire_Rate"]?)
      )
    end
  end

  private def insert_magazines(db : DB::Database, magazines : Array(Hash(String, JsonValue)))
    magazines.each do |mag|
      db.exec(
        "INSERT INTO magazines (name, category, magazine_size, reload_time, damage_mod, fire_rate_mod) VALUES (?, ?, ?, ?, ?, ?)",
        mag["Name"].as(String),
        mag["Category"].as(String),
        json_value_to_f(mag["Magazine_Size"]?),
        json_value_to_f(mag["Reload_Time"]?),
        json_value_to_f(mag["Damage"]?),
        json_value_to_f(mag["Fire_Rate"]?)
      )
    end
  end

  private def insert_parts(db : DB::Database, part_type : String, parts : Array(Hash(String, JsonValue)))
    parts.each do |part|
      db.exec(
        "INSERT INTO parts (part_type, name, category, damage_mod, fire_rate_mod) VALUES (?, ?, ?, ?, ?)",
        part_type,
        part["Name"].as(String),
        part["Category"].as(String),
        json_value_to_f(part["Damage"]?),
        json_value_to_f(part["Fire_Rate"]?)
      )
    end
  end

  private def extract_damage_pair(value : JsonValue?) : Tuple(Float64, Float64)
    return {0.0, 0.0} unless value
    return {json_value_to_f(value), json_value_to_f(value)} unless value.is_a?(Array)

    values = value.as(Array(JsonValue))
    return {0.0, 0.0} if values.empty?
    return {json_value_to_f(values[0]), json_value_to_f(values[0])} if values.size == 1
    {json_value_to_f(values[0]), json_value_to_f(values[1])}
  end

  private def json_value_to_f(value : JsonValue?) : Float64
    return 0.0 unless value

    case value
    when Int32
      value.to_f
    when Int64
      value.to_f
    when Float64
      value
    else
      0.0
    end
  end

  def build_full_data(parts_file : String = PARTS_V2_SHEET, cores_file : String = CORES_SHEET) : ExportData
    parts_data = PartsParser.new.parse_file(parts_file)
    cores_data = CoresParser.new.parse_file(cores_file)

    combined_data = {
      "Barrels"   => parts_data["Barrels"],
      "Magazines" => parts_data["Magazines"],
      "Grips"     => parts_data["Grips"],
      "Stocks"    => parts_data["Stocks"],
      "Cores"     => cores_data,
    } of String => ItemRows

    ExportData.new(combined_data, CURRENT_PENALTIES, CURRENT_CATEGORIES)
  end

  def download_sheets
    SheetDownloader.new(SHEET_ID, SHEET_FOLDER).download([
      SheetExport.new(CORES_SHEET_GID, CORES_SHEET),
      SheetExport.new(PARTS_V2_SHEET_GID, PARTS_V2_SHEET),
    ])
  end

  def run
    download_sheets
    full_data = build_full_data
    save_sqlite(full_data, OUTPUT_FILE)
  end
end
