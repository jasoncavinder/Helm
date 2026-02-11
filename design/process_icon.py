import sys
from PIL import Image


def process_image(input_path, output_path):
    try:
        img = Image.open(input_path).convert("RGBA")
        datas = img.getdata()

        new_data = []
        for item in datas:
            # Check for black (allow some tolerance if needed, e.g. < 10)
            if item[0] < 10 and item[1] < 10 and item[2] < 10:
                new_data.append((255, 255, 255, 0))  # Transparent
            else:
                new_data.append(item)

        img.putdata(new_data)

        # Crop to content
        bbox = img.getbbox()
        if bbox:
            img = img.crop(bbox)

        img.save(output_path, "PNG")
        print(f"Successfully processed image to {output_path}")
    except Exception as e:
        print(f"Error processing image: {e}")
        sys.exit(1)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print("Usage: python process_icon.py <input> <output>")
        sys.exit(1)

    process_image(sys.argv[1], sys.argv[2])
