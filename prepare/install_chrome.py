import os
import platform
import urllib.request
import yaml
# https://storage.googleapis.com/chrome-for-testing-public/136.0.7103.113/mac-arm64/chrome-mac-arm64.zip

info="""
mac_arm64:
    chrome_url: https://storage.googleapis.com/chrome-for-testing-public/136.0.7103.113/mac-arm64/chrome-mac-arm64.zip
    driver_url: https://storage.googleapis.com/chrome-for-testing-public/136.0.7103.113/mac-arm64/chromedriver-mac-arm64.zip
"""
info=yaml.safe_load(info)




def get_cur_system():
    arch=platform.machine()
    system=platform.system()
    if arch == "arm64":
        if system == "Darwin":
            return "mac_arm64"

    raise ValueError(f"Unsupported architecture: {arch} on {system}")

# return cur_system
def download_chrome(info)->str:
    cur_system = get_cur_system()

    chrome_url = info[cur_system]["chrome_url"]
    driver_url = info[cur_system]["driver_url"]
    
    # prepare cache dir
    if not os.path.exists("prepare_cache"):
        os.makedirs("prepare_cache", exist_ok=True)
    
    # download chrome
    chrome_path = os.path.join("prepare_cache", "chrome.zip")
    driver_path = os.path.join("prepare_cache", "driver.zip")
    
    # download chrome
    if not os.path.exists(chrome_path):
        # use native urllib library
        print(f"\nDownloading Chrome from {chrome_url}")
        # urllib.request.urlretrieve(chrome_url, chrome_path)
        os.system(f"curl -o {chrome_path} {chrome_url}")
    
    # download driver
    if not os.path.exists(driver_path):
        # use native urllib library
        print(f"\nDownloading ChromeDriver from {driver_url}")
        # urllib.request.urlretrieve(driver_url, driver_path)
        os.system(f"curl -o {driver_path} {driver_url}")

    # unzip chrome
    return cur_system

def open_file_window(path:str):
    if platform.system() == "Darwin":
        os.system(f"open {path}")
    else:
        os.system(f"start {path}")

def print_highlight(text:str):
    # green
    print(f"\033[92m{text}\033[0m")

def install_chrome_mac(cur_system:str):
    ok_systems=['mac_arm64']
    if cur_system not in ok_systems:
        raise ValueError(f"Unsupported architecture: {cur_system}")
    
    # skip if Google Chrome for Testing.app in /Applications
    if os.path.exists("/Applications/Google Chrome for Testing.app"):
        print_highlight("Google Chrome for Testing.app already exists in /Applications")
        return
    
    if not os.path.exists(CHROME_UNZIP_DIR):
        os.system("unzip prepare_cache/chrome.zip -d prepare_cache/")

    # link chrome-mac-arm64/Applications to /Applications
    os.system(f"ln -s /Applications {os.path.join(CHROME_UNZIP_DIR, 'Applications')}")

    # find app in CHROME_UNZIP_DIR
    app=""
    for file in os.listdir(CHROME_UNZIP_DIR):
        if file.endswith(".app"):
            app = file
            break

    open_file_window(CHROME_UNZIP_DIR)
    
    print_highlight(f"Install the chrome manully by copy the 'Google Chrome for Testing.app' folder to /Applications")
    
def install_chrome_driver_mac(CHROME_DRIVER_UNZIP_DIR:str):    
    # cp chromedriver to prepare_cache/chromedriver
    os.system(f"cp {CHROME_DRIVER_UNZIP_DIR}/chromedriver prepare_cache/chromedriver")
    
    


def install(cur_system:str):
    CHROME_UNZIP_DIR = ""
    CHROME_DRIVER_UNZIP_DIR = ""
    if cur_system == "mac_arm64":
        CHROME_UNZIP_DIR = "prepare_cache/chrome-mac-arm64"
        CHROME_DRIVER_UNZIP_DIR = "prepare_cache/chromedriver-mac-arm64"
    else:
        raise ValueError(f"Unsupported architecture: {cur_system}")
        

    # unzip things
    if not os.path.exists(CHROME_UNZIP_DIR):
        os.system("unzip prepare_cache/chrome.zip -d prepare_cache/")

    if not os.path.exists(CHROME_DRIVER_UNZIP_DIR):
        os.system("unzip prepare_cache/driver.zip -d prepare_cache/")

    # install chrome
    install_chrome_mac(cur_system)

    install_chrome_driver_mac(CHROME_DRIVER_UNZIP_DIR)
        

if __name__ == "__main__":
    os.chdir(os.path.dirname(os.path.abspath(__file__)))
    
    cur_system = download_chrome(info)

    install(cur_system)


    
    
    
    
    
    
    
