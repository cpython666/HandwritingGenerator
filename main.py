import sys
from PyQt6.QtWidgets import (QApplication, QGraphicsView, QGraphicsScene,QWidget,QHBoxLayout,QVBoxLayout,QLabel,
                             QGraphicsPixmapItem, QGraphicsRectItem, QMainWindow,QFormLayout,QSpinBox,QDoubleSpinBox,
                             QGraphicsTextItem, QPushButton,QInputDialog,QFontComboBox,QComboBox,
                             QSlider,QVBoxLayout,QFileDialog,QColorDialog,QTextEdit)
from PyQt6.QtCore import Qt, QRectF, Qt
from PyQt6.QtGui import QPixmap, QPainter, QImage, QFont, QKeySequence,QShortcut,QFont, QFontDatabase,QColor,QIcon

class HandFontWindow(QMainWindow):
    def __init__(self):
        super().__init__()

        self.initUI()

    def initUI(self):
        self.setGeometry(100, 100, 1200, 600)
        self.setWindowTitle('手写字体图片生成器')
        self.setWindowIcon(QIcon('logo.png'))

        self.initView()
        self.config_widget=self.initConfigForm()

        # 将配置窗口添加到主窗口的布局中
        main_layout = QHBoxLayout()
        main_layout.addWidget(self.config_widget)
        main_layout.addWidget(self.view)
        main_widget = QWidget(self)
        main_widget.setLayout(main_layout)
        self.setCentralWidget(main_widget)

        # 添加导出图片的快捷键 Ctrl + S
        export_shortcut = QShortcut(QKeySequence(QKeySequence.StandardKey.Save), self)
        export_shortcut.activated.connect(self.view.exportSceneToImage)

        self.updateView()

        self.show()
    
    def initView(self):
        self.scene = QGraphicsScene(self)
        self.view = GraphicsView(self.scene, self)
        self.setCentralWidget(self.view)

    def initConfigForm(self):
        # 创建配置参数窗口
        config_widget = QWidget(self)
        config_layout = QVBoxLayout(config_widget)
        # 添加文字区域配置
        text_config_label = QLabel('效果配置：')
        config_layout.addWidget(text_config_label)
        # 选择背景图片
        self.label_bg=QLabel('letter.png')
        self.button_bg=QPushButton('选择背景图')
        self.button_bg.clicked.connect(self.show_file_dialog)
        self.bg_path='./bgs/letter.png'
        config_layout.addWidget(self.label_bg)
        config_layout.addWidget(self.button_bg)
        # 选择字体
        self.label_font = QLabel("选择字体：")
        self.combo_box = QComboBox(self)
        self.combo_box.addItem('hand.ttf')
        self.combo_box.addItem('李国夫董事长手写体.ttf')
        self.combo_box.addItem('品如手写体.ttf')
        self.combo_box.addItem('青叶手写体.ttf')
        self.combo_box.addItem('上首鸿志手写体.ttf')
        self.combo_box.addItem('手写大象体.ttf')
        self.combo_box.addItem('未知手写体_1.ttf')
        self.combo_box.addItem('薛文轩钢笔楷体.ttf')
        self.combo_box.addItem('张维镜手写楷书.ttf')
        config_layout.addWidget(self.label_font)
        config_layout.addWidget(self.combo_box)
        self.combo_box.currentIndexChanged.connect(self.updateView)
        # 选择字体颜色
        self.label_font_color = QLabel("当前字体颜色：black")
        self.button_font_color = QPushButton('选择字体颜色')
        self.font_color='black'
        config_layout.addWidget(self.label_font_color)
        config_layout.addWidget(self.button_font_color)
        self.button_font_color.clicked.connect(self.show_color_dialog)

        self.label_text=QLabel('要显示的文字:')
        self.text_edit = QTextEdit(self)
        self.text_edit.setPlaceholderText('要显示的文字')
        self.text_edit.setText('''花费了一天，跟gpt3.5对话进百句，修修改改，最终做成这样的效果。
我的字迹一直很差，我也很讨厌写字，我花费了很多时间在练字上但我的字迹不好反坏。所以当我看到生成手写字图片的可能时，我便开始了迫不及待地尝试。
做到一半的时候我突然想到网上有没有已经做出来的。确实有，有点小失望，但又细看了一下发现它的应该操作比我的复杂点，所以我又继续做完就是现在的效果。''')
        # text_edit.toPlainText()
        self.text_edit.textChanged.connect(self.updateView)
        config_layout.addWidget(self.label_text)
        config_layout.addWidget(self.text_edit)

        # 坐标信息
        # 创建滑块调节数值并为其设置标签显示当前数值
        self.slider_x = QSlider(Qt.Orientation.Horizontal)
        self.slider_x.setRange(0, 500)
        self.slider_x.setSingleStep(1)
        self.slider_x.valueChanged.connect(self.updateView)
        self.slider_x.setValue(0)
        self.label_x = QLabel("文字坐标x：0")
        config_layout.addWidget(self.label_x)
        config_layout.addWidget(self.slider_x)

        self.slider_y = QSlider(Qt.Orientation.Horizontal)
        self.slider_y.setRange(0, 800)
        self.slider_y.setSingleStep(1)
        self.slider_y.valueChanged.connect(self.updateView)
        self.slider_x.setValue(0)
        self.label_y = QLabel("文字坐标y：0")
        config_layout.addWidget(self.label_y)
        config_layout.addWidget(self.slider_y)

        # 文字区域宽度
        self.slider_width = QSlider(Qt.Orientation.Horizontal)
        self.slider_width.setRange(200, 2000)
        self.slider_width.setSingleStep(1)
        self.slider_width.setValue(500)
        self.slider_width.valueChanged.connect(self.updateView)
        self.label_width = QLabel("文字显示宽度：500")
        config_layout.addWidget(self.label_width)
        config_layout.addWidget(self.slider_width)        
        
        self.slider_height = QSlider(Qt.Orientation.Horizontal)
        self.slider_height.setRange(500, 2000)
        self.slider_height.setSingleStep(1)
        self.slider_height.setValue(500)
        self.slider_height.valueChanged.connect(self.updateView)
        self.label_height = QLabel("文字显示高度：500")
        config_layout.addWidget(self.label_height)
        config_layout.addWidget(self.slider_height)

        # 字体大小
        self.slider_font_size = QSlider(Qt.Orientation.Horizontal)
        self.slider_font_size.setRange(1, 100)
        self.slider_font_size.setSingleStep(1)
        self.slider_font_size.setValue(16)
        self.slider_font_size.valueChanged.connect(self.updateView)
        self.label_font_size = QLabel("文字大小：16")
        config_layout.addWidget(self.label_font_size)
        config_layout.addWidget(self.slider_font_size)
        # 字体粗细
        self.slider_font_weight = QSlider(Qt.Orientation.Horizontal)
        self.slider_font_weight.setRange(100, 1000)
        self.slider_font_weight.setSingleStep(100)
        self.slider_font_weight.setValue(500)
        self.slider_font_weight.valueChanged.connect(self.updateView)
        self.label_font_weight = QLabel("文字粗细：500")
        config_layout.addWidget(self.label_font_weight)
        config_layout.addWidget(self.slider_font_weight)

        # 字体间距
        self.slider_font_spacing = QSlider(Qt.Orientation.Horizontal)
        self.slider_font_spacing.setRange(-20,20)
        self.slider_font_spacing.setSingleStep(1)
        self.slider_font_spacing.setValue(2)
        self.slider_font_spacing.valueChanged.connect(self.updateView)
        self.label_font_spacing = QLabel("字体间距：0.1")
        config_layout.addWidget(self.label_font_spacing)
        config_layout.addWidget(self.slider_font_spacing)

        # 行间距
        self.slider_line_spacing = QSlider(Qt.Orientation.Horizontal)
        self.slider_line_spacing.setRange(20,80)
        self.slider_line_spacing.setSingleStep(1)
        self.slider_line_spacing.setValue(40)
        self.slider_line_spacing.valueChanged.connect(self.updateView)
        self.label_line_spacing = QLabel("行间距：1")
        config_layout.addWidget(self.label_line_spacing)
        config_layout.addWidget(self.slider_line_spacing)

        # 添加导出按钮
        export_btn = QPushButton('导出图片：', self)
        export_btn.clicked.connect(self.view.exportSceneToImage)
        config_layout.addWidget(export_btn)
        config_widget.setLayout(config_layout)

        return config_widget
    
    def show_file_dialog(self):
        # 创建文件选择框
        file_dialog = QFileDialog(self)
        file_dialog.setWindowTitle('Open File')
        file_dialog.setFileMode(QFileDialog.FileMode.ExistingFile)

        # 显示文件选择框并等待用户选择
        if file_dialog.exec() == QFileDialog.DialogCode.Accepted:
            # 获取用户选择的文件路径
            selected_file = file_dialog.selectedFiles()[0]
            self.label_bg.setText(selected_file)
            self.bg_path=selected_file
            self.updateView()
    def show_color_dialog(self):
        # 创建颜色选择框
        color_dialog = QColorDialog(self)
        color_dialog.setWindowTitle('Choose Color')

        # 设置初始颜色（可选）
        color_dialog.setCurrentColor(QColor(255, 0, 0))

        # 显示颜色选择框并等待用户选择
        if color_dialog.exec() == QColorDialog.DialogCode.Accepted:
            # 获取用户选择的颜色
            self.font_color = color_dialog.currentColor().name()
            self.label_font_color.setText('当前字体颜色：'+self.font_color)
            self.updateView()    

    def updateView(self):
        current_text = self.combo_box.currentText()
        font_path='./fonts/'+current_text
        self.label_x.setText(f"文字坐标x：{self.slider_x.value()}")
        self.label_y.setText(f"文字坐标y：{self.slider_y.value()}")
        self.label_font_size.setText(f"文字大小：{self.slider_font_size.value()}")
        self.label_font_weight.setText(f"文字粗细：{self.slider_font_weight.value()}")
        self.label_font_color.setText(f"当前字体颜色：{self.font_color}")
        self.label_font_spacing.setText(f"文字间隔：{round(self.slider_font_spacing.value()/20,2)}")
        self.label_line_spacing.setText(f"行间隔x：{round(self.slider_line_spacing.value()/20,2)}")
        self.label_width.setText(f"文字显示宽度：{self.slider_width.value()}")
        self.label_height.setText(f"文字显示高度：{self.slider_height.value()}")

        text_tmp=self.text_edit.toPlainText().replace(' ','&nbsp;').replace('\n','<br>')

        self.view.addTextItem(text=text_tmp, 
                        bg_path=self.bg_path,
                        font_path=font_path,
                        font_color=self.font_color,
                        font_weight=self.slider_font_weight.value(),
                        x=self.slider_x.value(),y=self.slider_y.value(),                        
                        font_size=self.slider_font_size.value(),
                        font_spacing=round(self.slider_font_spacing.value()/20,2),
                        line_spacing=round(self.slider_line_spacing.value()/40,2),
                        rect_width=self.slider_width.value(), 
                        rect_height=self.slider_height.value()
                        )

class GraphicsView(QGraphicsView):
    def __init__(self, scene, parent):
        super().__init__(scene, parent)
        
        self.setStyleSheet('GraphicsView{background-color:#FDF6E3;}')
        self.setRenderHint(QPainter.RenderHint.Antialiasing, True)
        self.setDragMode(QGraphicsView.DragMode.ScrollHandDrag)  # 启用拖拽模式

        self.drawing_rect = None
        self.origin = None
        self.is_drawing = False
        self.coordinate_text_items = {}  # 存储坐标信息的字典，以矩形为键

    def wheelEvent(self, event):
        modifiers = event.modifiers()
        if modifiers == Qt.KeyboardModifier.ControlModifier:
            # 只有在按下 Ctrl 键时才缩放
            factor = 1.2  # 缩放因子
            if event.angleDelta().y() < 0:
                factor = 1.0 / factor  # 对于负的滚轮事件进行缩小
            self.scale(factor, factor)

    def mouseDoubleClickEvent(self, event):
        super().mouseDoubleClickEvent(event)
        if event.button() == Qt.MouseButton.LeftButton:
            # 获取鼠标双击的位置
            double_click_position = event.pos()
            # 获取双击位置相对于原图的像素位置
            double_click_scene_position = self.mapToScene(double_click_position)
            # 获取点击位置相对于原图的像素位置
            item = self.scene().itemAt(double_click_scene_position, self.transform())
            if isinstance(item, QGraphicsRectItem):
                # 删除被点击的矩形的坐标信息
                rect_key = id(item)
                if rect_key in self.coordinate_text_items:
                    self.removeCoordinateTextItems(self.coordinate_text_items[rect_key])
                # 删除被点击的矩形
                self.scene().removeItem(item)

    def mouseMoveEvent(self, event):
        super().mouseMoveEvent(event)

        if event.buttons() & Qt.MouseButton.RightButton:
            if self.is_drawing and self.origin is not None:
                # 获取鼠标当前位置
                current_position = event.pos()

                # 获取当前位置相对于原图的像素位置
                current_scene_position = self.mapToScene(current_position)

                # 更新矩形的大小
                self.drawing_rect.setRect(QRectF(self.origin, current_scene_position))

        # 实时显示图像
        scene_rect = self.sceneRect()
        self.setSceneRect(scene_rect)

    def mouseReleaseEvent(self, event):
        super().mouseReleaseEvent(event)

        if event.button() == Qt.MouseButton.RightButton:
            if self.is_drawing and self.origin is not None:
                self.is_drawing = False

                # 获取鼠标释放的位置
                release_position = event.pos()

                # 获取释放位置相对于原图的像素位置
                release_scene_position = self.mapToScene(release_position)

                # 更新矩形的大小
                self.drawing_rect.setRect(QRectF(self.origin, release_scene_position))

                # 显示矩形的坐标信息
                self.showRectCoordinates(self.drawing_rect)

    def mousePressEvent(self, event):
        super().mousePressEvent(event)

        if event.button() == Qt.MouseButton.RightButton:
            if not self.is_drawing:
                # 获取鼠标点击的位置
                click_position = event.pos()

                # 获取点击位置相对于原图的像素位置
                scene_position = self.mapToScene(click_position)
                self.origin = scene_position
                self.is_drawing = True

                # 创建一个新的矩形
                self.drawing_rect = QGraphicsRectItem(QRectF(self.origin, self.origin))
                self.scene().addItem(self.drawing_rect)

    def showRectCoordinates(self, rect_item):
        # 获取矩形的左上角和右下角相对于图片的坐标
        rect_top_left = rect_item.rect().topLeft()
        rect_bottom_right = rect_item.rect().bottomRight()
        # 将坐标信息显示在矩形的左上角和右下角
        # 在矩形的左上角显示坐标信息
        text_item_top_left = QGraphicsTextItem(f"({int(rect_top_left.x())}, {int(rect_top_left.y())})")
        text_item_top_left.setFont(QFont('Arial', 10))
        text_item_top_left.setDefaultTextColor(Qt.GlobalColor.red)
        text_item_top_left.setPos(rect_top_left.x() + 5, rect_top_left.y() - 20)
        self.scene().addItem(text_item_top_left)
        # 在矩形的右下角显示坐标信息
        text_item_bottom_right = QGraphicsTextItem(f"({int(rect_bottom_right.x())}, {int(rect_bottom_right.y())})")
        text_item_bottom_right.setFont(QFont('Arial', 10))
        text_item_bottom_right.setDefaultTextColor(Qt.GlobalColor.red)
        text_item_bottom_right.setPos(rect_bottom_right.x() - 50, rect_bottom_right.y() + 5)
        self.scene().addItem(text_item_bottom_right)
        # 将矩形对象作为键，将坐标信息文本项存储到字典中
        rect_key = id(rect_item)
        self.coordinate_text_items[rect_key] = [text_item_top_left, text_item_bottom_right]

    def removeCoordinateTextItems(self, text_items):
        # 从场景中删除坐标信息文本项
        for text_item in text_items:
            self.scene().removeItem(text_item)

        # 清空字典中的坐标信息
        text_items.clear()
    def addTextItem(self, text,
                    bg_path='./bgs/letter.png',
                    font_path="./fonts/hand.ttf",font_size=12, 
                    font_color='black',
                    font_weight=600,
                    x=100,y=100,
                    font_spacing=0, line_spacing=1.2, 
                    rect_width=500, rect_height=50):
        # 清除先前的文本项
        self.scene().clear()
        #加载背景图片
        pixmap = QPixmap(bg_path)
        item = QGraphicsPixmapItem(pixmap)
        self.scene().addItem(item)
        # 创建一个QGraphicsTextItem
        text_item = QGraphicsTextItem()
        text_item.setPos(x, y)
        # 加载本地字体文件,设置字体大小
        font_id = QFontDatabase.addApplicationFont(font_path)
        font_family = QFontDatabase.applicationFontFamilies(font_id)[0]
        font = QFont(font_family,font_size)
        text_item.setFont(font)
        # 使用 HTML 样式设置文本，包括换行和调整行间距、文字间距
        html_text = f"<div style='font-weight: {font_weight};color:{font_color};font-size:{font_size}pt; line-height:{line_spacing * 100}%; letter-spacing:{font_spacing}em;'>{text}</div>"
        text_item.setHtml(html_text)
        # 设置文本项的边界矩形
        text_item.setTextInteractionFlags(Qt.TextInteractionFlag.TextEditorInteraction)
        text_item.setTextWidth(rect_width)
        # 将文本项添加到场景中
        self.scene().addItem(text_item)

    def exportSceneToImage(self):
        # 获取场景的范围
        scene_rect = self.sceneRect()
         # 弹出输入对话框获取用户输入的文件名，默认为"img_1.png"
        file_name, _ = QInputDialog.getText(self, '导出图片', '输入文件名', text='img_1.png')
        # 如果用户取消了输入，则返回
        if not file_name:
            return
        # 创建一个QImage对象
        image = QImage(scene_rect.size().toSize(), QImage.Format.Format_ARGB32)
        image.fill(Qt.GlobalColor.white)
        # 创建一个QPainter对象
        painter = QPainter(image)
        # 将场景渲染到QImage中
        self.scene().render(painter, QRectF(image.rect()), scene_rect)
        # 结束绘制
        painter.end()
        # 保存QImage为图片文件
        image.save(file_name)

if __name__ == '__main__':
    app = QApplication(sys.argv)
    viewer = HandFontWindow()
    sys.exit(app.exec())