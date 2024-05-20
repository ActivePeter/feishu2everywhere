from selenium import webdriver
from selenium.webdriver import ActionChains
from selenium.webdriver.common.by import By
from selenium.webdriver.common.actions.wheel_input import ScrollOrigin
import time
import base64

options = webdriver.ChromeOptions()
options.add_argument("--disk-cache-size=0")
options.add_argument("--media-cache-size=0")
options.add_argument("--disable-gpu-shader-disk-cache")
options.add_argument(f"--user-data-dir=./user")
driver = webdriver.Chrome(options=options)

#设置窗口高度20000
# driver.set_window_size(1000, 20000)

driver.get("https://fvd360f8oos.feishu.cn/docx/Q3c6dJG5Go3ov6xXofZcGp43nfb")

time.sleep(1)

def visible_elem(elem):
    return driver.execute_script(
      "var elem = arguments[0],                 " +
      "  box = elem.getBoundingClientRect(),    " +
      "  cx = box.left + box.width / 2,         " +
      "  cy = box.top + box.height / 2,         " +
      "  e = document.elementFromPoint(cx, cy); " +
      "for (; e; e = e.parentElement) {         " +
      "  if (e === elem)                        " +
      "    return true;                         " +
      "}                                        " +
      "return false;                            "
      , elem)


img_repeat=0

with open("out.md","w") as outmd:
    def depth_pre(depth):
        if depth==0:
            return ""
        if depth==1:
            return "   "
        if depth==2:
            return "      "
        if depth==3:
            return "         "
        if depth==4:
            return "            "
        if depth==5:
            return "               "
        return "                  "
    
    APPEAR={}
    APPEAR_IMG={}
    def text_detail(element):
        try:
            line=element.find_element(by=By.CSS_SELECTOR, value=".ace-line")
            line_spans=line.find_elements(by=By.CSS_SELECTOR, value=":scope > span")
            text=""
            for span in line_spans:
                try:
                    ref=span.find_element(by=By.CSS_SELECTOR, value=".mention-doc")
                    href=ref.get_attribute("href")
                    alias=ref.text
                    text+=f"[{alias}]({href})"
                except:
                    try:
                        ref=span.find_element(by=By.CSS_SELECTOR, value=".link")
                        href=ref.get_attribute("href")
                        alias=ref.text
                        text+=f"[{alias}]({href})"
                    except:
                        try:
                            ref=span.find_element(by=By.CSS_SELECTOR, value=".inline-code")
                            text+=f"`{ref.text}`"
                        except:
                            # font-weight:bold;
                            if span.value_of_css_property("font-weight")=="bold":
                                text+="**"+span.text+"**"
                            else:
                                text+=span.text
            return text
        except:
            return "not line text"
    
    def append(element,depth):
        def write_with_depth(text,depth):
            # add space to each \n
            w=depth_pre(depth)+text.replace("\n","\n"+depth_pre(depth))
            # remove tail space
            # if last is space, append \n
            if w[-1]==" ":
                w=w[:-1]+"\n"
            print(depth,w)
            outmd.write(w)
        

        eclass=element.get_attribute("class")
        textfmt=text_detail(element)
        # replace first \n to space
        ordertext=element.text.strip().replace("\n"," ",1)
        id=element.get_attribute("data-record-id")
        

        succ=True
        if eclass.find("docx-heading1-block")!=-1:
            write_with_depth("# "+textfmt+"\n\n",depth)
        elif eclass.find("docx-heading2-block")!=-1:
            write_with_depth("## "+textfmt+"\n\n",depth)
        elif eclass.find("docx-text-block")!=-1:
            write_with_depth(textfmt+"\n\n",depth)
        elif eclass.find("docx-code-block")!=-1:
            write_with_depth("```\n"+element.text+"\n```\n\n",depth)
        elif eclass.find("docx-ordered-block")!=-1:
            # outmd.write(ordertext+"\n\n")
            append_list(element,depth)
        elif eclass.find("docx-unordered-block")!=-1:
            write_with_depth(ordertext+"\n\n",depth)
        elif eclass.find("docx-todo-block")!=-1:
            write_with_depth("- "+element.text.strip()+"\n\n",depth)
        elif eclass.find("docx-whiteboard-block")!=-1 or \
            eclass.find("docx-synced_source-block")!=-1:
            succ=False
            #canvas
            try:
                canvas=element.find_element(by=By.CSS_SELECTOR, value="canvas")
                # get the canvas as a PNG base64 string
                canvas_base64 = driver.execute_script("return arguments[0].toDataURL('image/png').substring(21);", canvas)
                # decode
                canvas_png = base64.b64decode(canvas_base64)
                
                if id not in APPEAR_IMG:
                    img_repeat=0
                else:
                    img_repeat=APPEAR_IMG[id]
                
                with open(f"canvas{id}.png", 'wb') as f:
                    f.write(canvas_png)
                    
                if id not in APPEAR_IMG:
                    write_with_depth("![canvas](canvas"+id+".png)\n\n",depth)
                    APPEAR_IMG[id]=1
            except:
                print("canvas not found")
        else:
            write_with_depth(eclass+":"+element.text.strip()+"\n\n",depth)
        if succ:
            APPEAR[id]=True
        return succ

    def append_list(listblock,depth):
        # .block > list-wrapper > .list
        # .block > list-wrapper > .list-children
        list= listblock.find_element(by=By.CSS_SELECTOR, value=".list-wrapper > .list")
        # listtext=list.text.replace("\n"," ",1)
        listtext=text_detail(list)
        outmd.write(depth_pre(depth)+listtext+"\n\n")

        try:
            list_children= listblock.find_element(by=By.CSS_SELECTOR, value=".list-wrapper > .list-children")
            child_elems=list_children.find_elements(by=By.CSS_SELECTOR, value=":scope > .render-unit-wrapper > .block")
            for e in child_elems:
                append(e,depth+1)
        except:
            pass
            

    
    # collect=[]
    def collect_elements():
        root_css=".root-render-unit-container > .render-unit-wrapper > .block"
        elements = driver.find_elements(by=By.CSS_SELECTOR, value=root_css)
        newcnt=0
        for e in elements:
            # print class
            # print("element",e,e.get_attribute("class"))
            block_id=e.get_attribute("data-record-id")
            eclass=e.get_attribute("class")
            # block_id = e.get_attribute("data-block-id")
            if eclass.find("docx-whiteboard-block")!=-1 or \
                eclass.find("docx-synced_source-block")!=-1 or \
                block_id not in APPEAR:
                if append(e,0):                
                    newcnt+=1
        print("collect_elements",newcnt)
        return newcnt

    import threading
    def timeout(task,t):
        # 创建线程
        thread = threading.Thread(target=task)
        # 启动线程
        thread.start()
        # 等待最多 5 秒
        thread.join(timeout=t)


    nonewtime=0
    while True:
        container = driver.find_element(by=By.CSS_SELECTOR, value=".render-unit-wrapper ")
        newcnt=collect_elements()

        
        # scroll y+h
        scroll_origin = ScrollOrigin.from_element(container, 0, 0)
        def scroll():
            ActionChains(driver)\
                .scroll_from_origin(scroll_origin, 0, 250)\
                .perform()
        print("begin scroll")
        timeout(scroll, 1)
        print("end scroll")


        # driver.execute_script("arguments[0].scrollIntoView();", container)
        time.sleep(3)
        if newcnt==0:
            nonewtime+=1
            if nonewtime>5:
                break
        else:
            nonewtime=0
        print("continue collect elements")


    outmd.close()

    while True:
        time.sleep(1000)